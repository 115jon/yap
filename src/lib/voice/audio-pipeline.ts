import { clog } from "@/lib/console-logger";

const audioLog = clog("SFU:Audio");
// ============================================================================
// AudioPipeline — Volume control, master gain, limiter, output device
//
// Chain: MediaStreamSource → per-track GainNode → StereoPanner
//        [→ boost limiter, only while that track's gain is above 1]
//        → Limiter → MasterGain → destination
// ============================================================================

import { resumeSoundContext } from "@/lib/sounds";
import { prewarmCaptureAudioContext } from "@/lib/voice/capture-audio-context";
import {
  BOOST_CEILING_DBFS,
  BOOST_LIMITER_PROCESSOR,
} from "@/lib/voice/boost-limiter.worklet.js";
import { MAX_PEER_GAIN } from "@/lib/voice/peer-volume";
import { vadWorkletUrl } from "@/lib/voice/vad";

// ── Boost limiter ─────────────────────────────────────────────────────────
// A person turned up past 100 % (up to 400 %) gets a limiter of their own,
// after their gain and pan and before the shared mix limiter, so a boosted
// shout is held at the ceiling instead of driving the mix limiter (which
// would duck everyone else). The limiter is the "rm-boost-limiter"
// AudioWorklet (boost-limiter.worklet.js: native parity with native/docs/
// LIBRARIES.md §11.1, a 1.5 ms look-ahead, 5 ms hold, 60 ms release, ceiling
// -3 dBFS). Until its module has loaded, or where AudioWorklet is missing, a
// DynamicsCompressorNode set as a limiter stands in; it is stereo linked too
// but lets a hard onset overshoot its ceiling by a few dB.

/** The ceiling: the mix limiter's threshold, so one boosted person passes it untouched. */
const BOOST_CEILING_DB = BOOST_CEILING_DBFS;
const BOOST_RATIO = 20;
const BOOST_ATTACK_S = 0.0015;
const BOOST_RELEASE_S = 0.06;
/**
 * The compressor adds automatic make-up gain (Web Audio spec:
 * `(1 / fullRangeGain) ^ 0.6`, fullRangeGain being its curve's gain for a
 * 0 dBFS input). The trim after it takes that back out, so under the ceiling
 * the stand-in is unity and a boost is exactly the gain asked for.
 */
const BOOST_FULL_RANGE_GAIN = Math.pow(
  10,
  (BOOST_CEILING_DB - BOOST_CEILING_DB / BOOST_RATIO) / 20,
);
export const BOOST_TRIM = Math.pow(BOOST_FULL_RANGE_GAIN, 0.6);
/**
 * A track that comes back to 100 % or less keeps its limiter this long, so
 * the gain's 0.1 s glide down and the limiter's release finish before the
 * track returns to the plain path.
 */
export const BOOST_LIMITER_LINGER_MS = 600;

export interface BoostLimiter {
  kind: "worklet" | "compressor";
  input: AudioNode;
  output: AudioNode;
  dispose(): void;
}

/**
 * The AudioWorklet module. Vite emits it as a static asset; the literal
 * `new URL(..., import.meta.url)` form is what it rewrites.
 */
export function boostLimiterWorkletUrl(): string {
  return new URL("./boost-limiter.worklet.js", import.meta.url).href;
}

const boostWorkletReady = new WeakSet<BaseAudioContext>();
const boostWorkletLoads = new WeakMap<BaseAudioContext, Promise<boolean>>();

/** Load the boost limiter's worklet once per context; resolves whether it can be used. */
export function loadBoostLimiterWorklet(
  ctx: BaseAudioContext,
): Promise<boolean> {
  let load = boostWorkletLoads.get(ctx);
  if (!load) {
    load =
      typeof AudioWorkletNode !== "undefined" &&
      typeof ctx.audioWorklet?.addModule === "function"
        ? ctx.audioWorklet.addModule(boostLimiterWorkletUrl()).then(
            () => {
              boostWorkletReady.add(ctx);
              return true;
            },
            (err: unknown) => {
              audioLog.warn(
                "Boost limiter worklet unavailable; using a compressor",
                err,
              );
              return false;
            },
          )
        : Promise.resolve(false);
    boostWorkletLoads.set(ctx, load);
  }
  return load;
}

/**
 * The per-track boost limiter: the worklet once its module is loaded on
 * `ctx` (`loadBoostLimiterWorklet`), otherwise the compressor stand-in.
 */
export function createBoostLimiter(ctx: BaseAudioContext): BoostLimiter {
  if (boostWorkletReady.has(ctx)) {
    try {
      const node = new AudioWorkletNode(ctx, BOOST_LIMITER_PROCESSOR, {
        numberOfInputs: 1,
        numberOfOutputs: 1,
        outputChannelCount: [2],
        channelCount: 2,
        channelCountMode: "explicit",
        channelInterpretation: "speakers",
      });
      return {
        kind: "worklet",
        input: node,
        output: node,
        dispose: () => {
          node.port.postMessage({ type: "stop" });
          node.disconnect();
        },
      };
    } catch (err) {
      audioLog.warn(
        "Boost limiter worklet node failed; using a compressor",
        err,
      );
    }
  }
  const limiter = ctx.createDynamicsCompressor();
  limiter.threshold.value = BOOST_CEILING_DB;
  limiter.knee.value = 0;
  limiter.ratio.value = BOOST_RATIO;
  limiter.attack.value = BOOST_ATTACK_S;
  limiter.release.value = BOOST_RELEASE_S;
  const trim = ctx.createGain();
  trim.gain.value = BOOST_TRIM;
  limiter.connect(trim);
  return {
    kind: "compressor",
    input: limiter,
    output: trim,
    dispose: () => {
      limiter.disconnect();
      trim.disconnect();
    },
  };
}

// ── Audio Context Prewarming ──────────────────────────────────────────────
// Chrome requires AudioContext.resume() to be called during a user gesture.
// For calls, the SFU connects asynchronously after a gateway event (outside
// any gesture). prewarmAudioContext() should be called from the Accept/Call
// button click handler so the context is created in "running" state.
let _prewarmedContext: AudioContext | null = null;
let _prewarmedContextCleanupTimer: ReturnType<typeof setTimeout> | null = null;
const PREWARMED_CONTEXT_LIFETIME_MS = 10_000;

function createPlaybackAudioContext(): AudioContext {
  return new AudioContext({ latencyHint: "interactive" });
}

function clearPrewarmedContextCleanupTimer(): void {
  if (_prewarmedContextCleanupTimer !== null) {
    clearTimeout(_prewarmedContextCleanupTimer);
    _prewarmedContextCleanupTimer = null;
  }
}

/** Dispose a prewarmed context that was not consumed by a voice session. */
export function disposePrewarmedAudioContext(): void {
  clearPrewarmedContextCleanupTimer();
  const context = _prewarmedContext;
  _prewarmedContext = null;
  if (context && context.state !== "closed") {
    context.close().catch(() => {});
  }
}

function takePrewarmedAudioContext(): AudioContext | null {
  const context = _prewarmedContext;
  if (!context || context.state === "closed") {
    disposePrewarmedAudioContext();
    return null;
  }
  clearPrewarmedContextCleanupTimer();
  _prewarmedContext = null;
  return context;
}

/**
 * Create and resume an AudioContext during a user gesture (click/tap).
 * The AudioPipeline will reuse this context instead of creating a new
 * suspended one. Call this from call Accept/Initiate and voice-join clicks.
 *
 * Also resumes the sound effects AudioContext and the outbound capture
 * context (noise gate / RNNoise) so a single user gesture unlocks ALL
 * audio — preventing the double "Interaction Required" modal.
 */
export function prewarmAudioContext(): void {
  try {
    if (!_prewarmedContext || _prewarmedContext.state === "closed") {
      disposePrewarmedAudioContext();
      _prewarmedContext = createPlaybackAudioContext();
    }
    clearPrewarmedContextCleanupTimer();
    _prewarmedContextCleanupTimer = setTimeout(
      disposePrewarmedAudioContext,
      PREWARMED_CONTEXT_LIFETIME_MS,
    );
    _prewarmedContext.resume().catch(() => {});
    // Unify: also resume the sound effects context during this gesture
    resumeSoundContext().catch(() => {});
    // And the outbound capture context the microphone gate is built on, so
    // the gate starts running instead of publishing a suspended graph.
    prewarmCaptureAudioContext([vadWorkletUrl()]);
  } catch {
    /* AudioContext unavailable */
  }
}

/** Callback to notify SFUClient of audio context state changes */
export interface AudioPipelineCallbacks {
  onAudioResumed(): void;
  onDirectAudioAttached?: (details: {
    participantId: string;
    trackName: string;
  }) => void;
  onAudioGraphConnected?: (details: {
    participantId: string;
    trackName: string;
  }) => void;
  onFirstAudible?: (details: { rms: number; preLimiterRms?: number }) => void;
  onProbeSample?: (details: {
    rms: number;
    preLimiterRms?: number;
    contextState: AudioContextState;
    elapsedMs: number;
  }) => void;
}

export class AudioPipeline {
  private volumeContext: AudioContext | null = null;
  private limiter: DynamicsCompressorNode | null = null;
  private masterGainNode: GainNode | null = null;
  private volumeLevels: Map<string, number> = new Map();
  private trackVolumeLevels: Map<string, number> = new Map();
  private trackPans: Map<string, number> = new Map();
  private masterVolumeLevel = 1;
  private outputDeviceId = "default";
  private directAudioElements: Map<string, Map<string, HTMLAudioElement>> =
    new Map();
  private volumeGains: Map<string, Map<string, GainNode>> = new Map();
  private spatialPanners: Map<string, Map<string, StereoPannerNode>> =
    new Map();
  /** Per-track boost limiters (`participantId::trackName`), only while gain > 1. */
  private boostLimiters: Map<string, BoostLimiter> = new Map();
  private boostLingerTimers: Map<string, ReturnType<typeof setTimeout>> =
    new Map();
  private volumeSources: Map<string, Map<string, MediaStreamAudioSourceNode>> =
    new Map();
  /** Muted <audio> elements that activate Chrome's remote track media pipeline */
  private activatorElements: Map<string, Map<string, HTMLAudioElement>> =
    new Map();
  /**
   * Tracks that were connected while the AudioContext was suspended.
   * Firefox silently drops audio from createMediaStreamSource() on a suspended
   * context (no buffering). When the context transitions to 'running' via user
   * gesture, we re-create the MediaStreamSource for each pending track.
   */
  private pendingTracks: Map<
    string,
    { participantId: string; trackName: string; stream: MediaStream }
  > = new Map();
  /** Whether we've already installed the onstatechange listener */
  private stateChangeInstalled = false;
  private preLimiterAnalyser: AnalyserNode | null = null;
  private preLimiterAnalyserData: Uint8Array | null = null;
  private outputAnalyser: AnalyserNode | null = null;
  private outputAnalyserData: Uint8Array | null = null;
  private firstAudibleReported = false;
  private firstAudibleAttempts = 0;
  private firstAudibleFrame: number | null = null;

  private readonly callbacks: AudioPipelineCallbacks;

  constructor(callbacks: AudioPipelineCallbacks) {
    this.callbacks = callbacks;
  }

  /**
   * Resume the volume AudioContext if it's suspended.
   * Call this from a user gesture (like clicking "Join").
   */
  async resumeAudioContext(): Promise<void> {
    if (!this.volumeContext || this.volumeContext.state === "closed") {
      // Direct remote sinks do not need an AudioContext. Only adopt a context
      // that was explicitly prewarmed during the join click.
      const prewarmedContext = takePrewarmedAudioContext();
      if (prewarmedContext) this.volumeContext = prewarmedContext;
      else {
        this.replayDirectAudioSinks();
        return;
      }
    }
    if (this.volumeContext.state === "suspended") {
      try {
        await this.volumeContext.resume();
        audioLog.info("AudioContext resumed successfully");
        this.callbacks.onAudioResumed();
      } catch (err) {
        audioLog.warn("Failed to resume AudioContext:", err);
      }
    }
    // Also resume the sound effects context so both are unlocked together
    resumeSoundContext().catch(() => {});
    this.replayDirectAudioSinks();
  }

  private replayDirectAudioSinks(): void {
    this.directAudioElements.forEach((elements) => {
      elements.forEach((element) => {
        void element.play().catch(() => {});
      });
    });
  }

  /**
   * Check if the AudioContext is currently suspended (blocked by browser).
   */
  isAudioSuspended(): boolean {
    return this.volumeContext?.state === "suspended";
  }

  /**
   * Set the volume for a specific remote participant (0.0 = mute, 1.0 = normal,
   * max 4.0 = 400 %). Above 1.0 each of their tracks plays through its own
   * boost limiter (`createBoostLimiter`).
   */
  setParticipantVolume(participantId: string, level: number): void {
    const clamped = Math.max(0, Math.min(level, MAX_PEER_GAIN));
    this.volumeLevels.set(participantId, clamped);
    const gains = this.volumeGains.get(participantId);
    if (gains) {
      gains.forEach((gn, trackName) => {
        this.routeBoost(participantId, trackName);
        gn.gain.setTargetAtTime(
          this.getTrackGain(participantId, trackName),
          this.volumeContext?.currentTime || 0,
          0.1,
        );
      });
    }
    const directElements = this.directAudioElements.get(participantId);
    for (const trackName of [...(directElements?.keys() ?? [])]) {
      const element = directElements?.get(trackName);
      if (!element) continue;
      if (this.getEffectiveDirectVolume(participantId, trackName) > 1) {
        this.promoteDirectTrackWhenRunning(participantId, trackName);
      } else {
        element.volume = this.getDirectElementVolume(participantId, trackName);
      }
    }
  }

  /**
   * Sets volume for a specific track of a participant (clamped to 0.0–4.0;
   * above 1.0 it plays through its own boost limiter).
   */
  setTrackVolume(
    participantId: string,
    trackName: string,
    level: number,
  ): void {
    const clamped = Math.max(0, Math.min(level, MAX_PEER_GAIN));
    this.trackVolumeLevels.set(`${participantId}::${trackName}`, clamped);
    const gains = this.volumeGains.get(participantId);
    const gn = gains?.get(trackName);
    if (gn) {
      this.routeBoost(participantId, trackName);
      gn.gain.setTargetAtTime(
        clamped,
        this.volumeContext?.currentTime || 0,
        0.1,
      );
    }
    const element = this.directAudioElements.get(participantId)?.get(trackName);
    if (element) {
      if (this.getEffectiveDirectVolume(participantId, trackName) > 1) {
        this.promoteDirectTrackWhenRunning(participantId, trackName);
      } else {
        element.volume = this.getDirectElementVolume(participantId, trackName);
      }
    }
  }

  /**
   * Returns the shared output context so local media can follow the same
   * gesture-resume and output-device path as voice tracks.
   */
  getAudioContext(): AudioContext {
    if (!this.volumeContext || this.volumeContext.state === "closed") {
      this.volumeContext =
        takePrewarmedAudioContext() ?? createPlaybackAudioContext();
    }
    return this.volumeContext;
  }

  setTrackPan(participantId: string, trackName: string, pan: number): void {
    const clamped = Math.max(-1, Math.min(pan, 1));
    this.trackPans.set(`${participantId}::${trackName}`, clamped);
    if (
      clamped !== 0 &&
      this.directAudioElements.get(participantId)?.has(trackName)
    ) {
      this.promoteDirectTrackWhenRunning(participantId, trackName);
    }
    const node = this.spatialPanners.get(participantId)?.get(trackName);
    if (!node || !this.volumeContext) return;
    node.pan.setTargetAtTime(
      clamped,
      this.volumeContext.currentTime || 0,
      0.08,
    );
  }

  /** Get current volume level for a participant (defaults to 1.0). */
  getParticipantVolume(participantId: string): number {
    return this.volumeLevels.get(participantId) ?? 1.0;
  }

  /** Attach an incoming remote audio track to the browser's direct media sink. */
  attachRemoteAudioTrack(
    participantId: string,
    track: MediaStreamTrack,
    trackName: string,
  ): MediaStream {
    if (track.kind !== "audio") return new MediaStream([track]);

    const alreadyAttached = this.attachedRemoteStream(
      participantId,
      trackName,
      track,
    );
    if (alreadyAttached) return alreadyAttached;

    this.removeTrackVolume(participantId, trackName);
    const stream = new MediaStream([track]);
    if (typeof document === "undefined") return stream;

    const element = document.createElement("audio");
    element.autoplay = true;
    element.setAttribute("playsinline", "true");
    element.preload = "auto";
    element.srcObject = stream;
    element.volume = this.getDirectElementVolume(participantId, trackName);
    element.setAttribute("aria-hidden", "true");
    element.style.display = "none";
    document.body?.appendChild(element);

    let participantElements = this.directAudioElements.get(participantId);
    if (!participantElements) {
      participantElements = new Map();
      this.directAudioElements.set(participantId, participantElements);
    }
    participantElements.set(trackName, element);
    if (typeof track.addEventListener === "function") {
      track.addEventListener(
        "ended",
        () => {
          if (
            this.directAudioElements.get(participantId)?.get(trackName) ===
            element
          ) {
            this.removeDirectAudioTrack(participantId, trackName);
          }
        },
        { once: true },
      );
    }
    if (this.getEffectiveDirectVolume(participantId, trackName) > 1) {
      this.promoteDirectTrackWhenRunning(participantId, trackName);
      return stream;
    }
    void this.applyDirectOutputDevice(element);
    void element.play().catch(() => {});
    this.callbacks.onDirectAudioAttached?.({ participantId, trackName });
    audioLog.info("Remote audio direct sink attached", {
      participantId,
      trackName,
      attachedAt: Date.now(),
    });
    return stream;
  }

  private attachedRemoteStream(
    participantId: string,
    trackName: string,
    track: MediaStreamTrack,
  ): MediaStream | null {
    const element = this.directAudioElements.get(participantId)?.get(trackName);
    const elementStream = element?.srcObject;
    if (
      elementStream instanceof MediaStream &&
      elementStream.getAudioTracks().includes(track)
    ) {
      return elementStream;
    }
    const sourceStream = this.volumeSources
      .get(participantId)
      ?.get(trackName)?.mediaStream;
    if (sourceStream?.getAudioTracks().includes(track)) return sourceStream;
    return null;
  }

  private getDirectElementVolume(
    participantId: string,
    trackName?: string,
  ): number {
    return Math.max(
      0,
      Math.min(this.getEffectiveDirectVolume(participantId, trackName), 1),
    );
  }

  private getTrackGain(participantId: string, trackName: string): number {
    return (
      this.trackVolumeLevels.get(`${participantId}::${trackName}`) ??
      this.getParticipantVolume(participantId)
    );
  }

  private getEffectiveDirectVolume(
    participantId: string,
    trackName?: string,
  ): number {
    const gain = trackName
      ? this.getTrackGain(participantId, trackName)
      : this.getParticipantVolume(participantId);
    return Math.max(0, gain * this.masterVolumeLevel);
  }

  private getStoredPan(participantId: string, trackName: string): number {
    return this.trackPans.get(`${participantId}::${trackName}`) ?? 0;
  }

  private needsWebAudio(participantId: string, trackName: string): boolean {
    return (
      this.getEffectiveDirectVolume(participantId, trackName) > 1 ||
      this.getStoredPan(participantId, trackName) !== 0
    );
  }

  private removeDirectAudioTrack(
    participantId: string,
    trackName: string,
  ): void {
    const elements = this.directAudioElements.get(participantId);
    const element = elements?.get(trackName);
    if (!element) return;
    element.srcObject = null;
    element.remove();
    elements?.delete(trackName);
    if (elements?.size === 0) this.directAudioElements.delete(participantId);
  }

  private promoteDirectTrackWhenRunning(
    participantId: string,
    trackName: string,
  ): void {
    const ctx = this.getAudioContext();
    if (ctx.state === "running") {
      this.promoteDirectTrack(participantId, trackName);
      return;
    }
    const element = this.directAudioElements.get(participantId)?.get(trackName);
    if (element) {
      element.volume = this.getDirectElementVolume(participantId, trackName);
      void element.play().catch(() => {});
    }
    void ctx.resume().then(() => {
      if (this.volumeContext !== ctx || ctx.state !== "running") return;
      if (!this.directAudioElements.get(participantId)?.has(trackName)) return;
      if (!this.needsWebAudio(participantId, trackName)) return;
      this.promoteDirectTrack(participantId, trackName);
    });
  }

  private promoteDirectTrack(participantId: string, trackName: string): void {
    const element = this.directAudioElements.get(participantId)?.get(trackName);
    const stream = element?.srcObject;
    const track =
      stream instanceof MediaStream ? stream.getAudioTracks()[0] : null;
    if (!track) return;
    this.removeDirectAudioTrack(participantId, trackName);
    this.applyVolumeToTrack(participantId, track, trackName);
  }

  private async applyDirectOutputDevice(
    element: HTMLAudioElement,
  ): Promise<void> {
    const sinkElement = element as HTMLAudioElement & {
      setSinkId?: (id: string) => Promise<void>;
    };
    if (!sinkElement.setSinkId) return;
    const sinkId =
      this.outputDeviceId === "default" ||
      this.outputDeviceId.startsWith("native:")
        ? ""
        : this.outputDeviceId;
    try {
      await sinkElement.setSinkId(sinkId);
    } catch (err) {
      audioLog.warn("Failed to set direct output device:", err);
    }
  }

  applyVolumeToTrack(
    participantId: string,
    track: MediaStreamTrack,
    trackName: string,
  ): MediaStream {
    if (track.kind !== "audio") {
      return new MediaStream([track]);
    }

    this.removeDirectAudioTrack(participantId, trackName);

    if (!this.volumeContext) {
      // Reuse prewarmed context if available
      this.volumeContext =
        takePrewarmedAudioContext() ?? createPlaybackAudioContext();
    }

    const ctx = this.volumeContext;
    ctx.resume().catch(() => {});
    void this.setOutputDevice(this.outputDeviceId);
    // Ready before anyone is turned up past 100 %.
    void loadBoostLimiterWorklet(ctx);

    // Create or reuse the shared limiter (one per AudioContext).
    // This is a DynamicsCompressorNode configured as a brick-wall limiter:
    // high threshold + hard knee + extreme ratio = only catches near-clipping
    // peaks, preserving natural dynamics (quiet audio stays quiet).
    if (!this.limiter) {
      const lim = ctx.createDynamicsCompressor();
      lim.threshold.value = -3; // only engage at -3 dBFS (near clipping)
      lim.knee.value = 0; // hard knee — brick-wall behavior
      lim.ratio.value = 20; // near-infinite compression above threshold
      lim.attack.value = 0.001; // 1 ms — catch transients instantly
      lim.release.value = 0.1; // 100 ms — recover fast, don't color the sound

      // Insert master gain between limiter and destination
      if (!this.masterGainNode) {
        this.masterGainNode = ctx.createGain();
        this.masterGainNode.gain.value = this.masterVolumeLevel;
        this.masterGainNode.connect(ctx.destination);
      }
      lim.connect(this.masterGainNode);
      this.limiter = lim;
    }

    // ── Chrome audio track activator ─────────────────────────────────
    // Chrome won't decode/activate a remote WebRTC track's media pipeline
    // until the track is attached to a media element (<audio> or <video>).
    // createMediaStreamSource() alone doesn't count as a consumer — it
    // reads zeros from the un-activated track. In voice channels the
    // ParticipantCard's <VideoPlayer> element serves as the activator,
    // but in calls (and any context without a visible media element for
    // audio tracks) we need an explicit one. Volume stays 0 so the audible
    // copy is the Web Audio graph. Do not set `muted`: Chrome treats a
    // muted-only consumer as idle and grows the RTP jitter buffer to ~1s.
    const activatorStream = new MediaStream([track]);
    const activator = document.createElement("audio");
    activator.srcObject = activatorStream;
    activator.volume = 0;
    activator.autoplay = true;
    activator.setAttribute("playsinline", "true");
    activator.setAttribute("aria-hidden", "true");
    activator.style.display = "none";
    document.body?.appendChild(activator);
    activator.play().catch(() => {});

    // Store activator for cleanup
    let participantActivators = this.activatorElements.get(participantId);
    if (!participantActivators) {
      participantActivators = new Map();
      this.activatorElements.set(participantId, participantActivators);
    }
    const existingActivator = participantActivators.get(trackName);
    if (existingActivator) {
      existingActivator.srcObject = null;
    }
    participantActivators.set(trackName, activator);

    const source = ctx.createMediaStreamSource(activatorStream);
    const gainNode = ctx.createGain();

    if (!this.outputAnalyser) {
      this.startFirstAudibleProbe(ctx, this.limiter!, this.masterGainNode!);
    }
    const pannerNode = ctx.createStereoPanner();
    pannerNode.pan.value = this.getStoredPan(participantId, trackName);

    gainNode.gain.value = this.getTrackGain(participantId, trackName);
    source.connect(gainNode);
    gainNode.connect(pannerNode);
    // A limiter left from this track's previous graph belonged to its old
    // panner.
    this.disposeBoost(participantId, trackName);
    if (this.getTrackGain(participantId, trackName) > 1) {
      this.insertBoost(participantId, trackName, pannerNode);
    } else {
      pannerNode.connect(this.limiter);
    }

    let participantGains = this.volumeGains.get(participantId);
    if (!participantGains) {
      participantGains = new Map();
      this.volumeGains.set(participantId, participantGains);
    }
    const existingGain = participantGains.get(trackName);
    if (existingGain) {
      existingGain.disconnect();
    }
    participantGains.set(trackName, gainNode);

    let participantPanners = this.spatialPanners.get(participantId);
    if (!participantPanners) {
      participantPanners = new Map();
      this.spatialPanners.set(participantId, participantPanners);
    }
    const existingPanner = participantPanners.get(trackName);
    if (existingPanner) {
      existingPanner.disconnect();
    }
    participantPanners.set(trackName, pannerNode);

    let participantSources = this.volumeSources.get(participantId);
    if (!participantSources) {
      participantSources = new Map();
      this.volumeSources.set(participantId, participantSources);
    }
    const existingSource = participantSources.get(trackName);
    if (existingSource) {
      existingSource.disconnect();
    }
    participantSources.set(trackName, source);

    // ── Firefox suspended-context fix ─────────────────────────────────
    // Firefox silently drops audio from createMediaStreamSource() when the
    // AudioContext is suspended (no buffering, no error — just silence).
    // This happens on auto-join flows where handleJoin() runs from a useEffect
    // with no user gesture, so ctx.resume() doesn't take effect.
    //
    // Solution: if the context is still suspended after our resume() attempt,
    // record this track as "pending". When the context eventually transitions
    // to 'running' (triggered by a user click/keypress via the resume listener
    // in useRoomVoiceChannel), we re-create the MediaStreamSource so audio
    // actually flows through the graph.
    if (ctx.state === "suspended") {
      const key = `${participantId}::${trackName}`;
      this.pendingTracks.set(key, {
        participantId,
        trackName,
        stream: activatorStream,
      });
      audioLog.warn(
        `AudioContext suspended — deferring audio connection for ${trackName} (will reconnect on resume)`,
      );
      this.installContextStateListener();
    }

    audioLog.info("Remote audio graph connected", {
      participantId,
      trackName,
      contextState: ctx.state,
      connectedAt: Date.now(),
    });
    this.callbacks.onAudioGraphConnected?.({ participantId, trackName });

    // Return the original stream; the AudioContext handles the audible output.
    return new MediaStream([track]);
  }

  /**
   * Put a track behind its boost limiter while its gain is above 1, and take
   * it back to the plain path (panner straight into the mix limiter)
   * `BOOST_LIMITER_LINGER_MS` after it comes back to 1 or less. Called before
   * the gain moves, so a rising gain is limited from its first sample.
   */
  private routeBoost(participantId: string, trackName: string): void {
    const key = `${participantId}::${trackName}`;
    const panner = this.spatialPanners.get(participantId)?.get(trackName);
    if (!panner || !this.limiter) return;
    const boost = this.boostLimiters.get(key);
    if (this.getTrackGain(participantId, trackName) > 1) {
      this.cancelBoostLinger(key);
      if (!boost) this.insertBoost(participantId, trackName, panner);
      return;
    }
    if (!boost || this.boostLingerTimers.has(key)) return;
    this.boostLingerTimers.set(
      key,
      setTimeout(() => {
        this.boostLingerTimers.delete(key);
        if (this.boostLimiters.get(key) !== boost) return;
        if (this.getTrackGain(participantId, trackName) > 1) return;
        const current = this.spatialPanners.get(participantId)?.get(trackName);
        if (current && this.limiter) {
          current.disconnect();
          current.connect(this.limiter);
        }
        this.disposeBoost(participantId, trackName);
      }, BOOST_LIMITER_LINGER_MS),
    );
  }

  private insertBoost(
    participantId: string,
    trackName: string,
    panner: StereoPannerNode,
  ): void {
    if (!this.volumeContext || !this.limiter) return;
    const ctx = this.volumeContext;
    const boost = createBoostLimiter(ctx);
    boost.output.connect(this.limiter);
    panner.disconnect();
    panner.connect(boost.input);
    this.boostLimiters.set(`${participantId}::${trackName}`, boost);
    if (boost.kind === "compressor") {
      void loadBoostLimiterWorklet(ctx).then((ready) => {
        if (ready) this.upgradeBoosts(ctx);
      });
    }
  }

  /** Swap compressor stand-ins for the worklet limiter once its module is loaded. */
  private upgradeBoosts(ctx: BaseAudioContext): void {
    if (this.volumeContext !== ctx || !this.limiter) return;
    for (const [key, boost] of this.boostLimiters) {
      if (boost.kind !== "compressor") continue;
      const [participantId = "", trackName = ""] = key.split("::");
      const panner = this.spatialPanners.get(participantId)?.get(trackName);
      if (!panner) continue;
      const next = createBoostLimiter(ctx);
      if (next.kind !== "worklet") {
        next.dispose();
        return;
      }
      next.output.connect(this.limiter);
      panner.disconnect();
      panner.connect(next.input);
      boost.dispose();
      this.boostLimiters.set(key, next);
    }
  }

  private cancelBoostLinger(key: string): void {
    const timer = this.boostLingerTimers.get(key);
    if (timer === undefined) return;
    clearTimeout(timer);
    this.boostLingerTimers.delete(key);
  }

  private disposeBoost(participantId: string, trackName: string): void {
    const key = `${participantId}::${trackName}`;
    this.cancelBoostLinger(key);
    const boost = this.boostLimiters.get(key);
    if (!boost) return;
    boost.dispose();
    this.boostLimiters.delete(key);
  }

  /**
   * Install an onstatechange listener on the AudioContext that re-connects
   * any pending audio sources when the context transitions to 'running'.
   * This is the Firefox fix: audio connected during suspension is silently dropped,
   * so we must re-create the MediaStreamSource once the context is live.
   */
  private installContextStateListener(): void {
    if (this.stateChangeInstalled || !this.volumeContext) return;
    this.stateChangeInstalled = true;

    this.volumeContext.onstatechange = () => {
      const ctx = this.volumeContext;
      if (!ctx || ctx.state !== "running" || this.pendingTracks.size === 0)
        return;

      audioLog.info(
        `AudioContext now running — reconnecting ${this.pendingTracks.size} deferred audio track(s)`,
      );

      for (const {
        participantId,
        trackName,
        stream,
      } of this.pendingTracks.values()) {
        // Verify the track is still alive (participant may have left)
        const audioTrack = stream.getAudioTracks()[0];
        if (!audioTrack || audioTrack.readyState !== "live") {
          audioLog.info(`Skipping dead deferred track: ${trackName}`);
          this.removeTrackVolume(participantId, trackName);
          continue;
        }

        try {
          // Disconnect the old (silent) source
          const oldSources = this.volumeSources.get(participantId);
          const oldSource = oldSources?.get(trackName);
          if (oldSource) {
            oldSource.disconnect();
          }

          // Re-create the source now that the context is running
          const newSource = ctx.createMediaStreamSource(stream);
          const gains = this.volumeGains.get(participantId);
          const gainNode = gains?.get(trackName);
          if (gainNode) {
            newSource.connect(gainNode);
          } else if (this.limiter) {
            // Fallback: connect directly to limiter
            newSource.connect(this.limiter);
          }

          // Update the source reference
          if (oldSources) {
            oldSources.set(trackName, newSource);
          }

          // Also re-trigger the activator play() now that we have a gesture
          const activators = this.activatorElements.get(participantId);
          const activator = activators?.get(trackName);
          if (activator) {
            activator.play().catch(() => {});
          }

          audioLog.info(`Reconnected deferred audio: ${trackName}`);
        } catch (err) {
          audioLog.warn(
            `Failed to reconnect deferred track ${trackName}:`,
            err,
          );
        }
      }

      this.pendingTracks.clear();
    };
  }

  /** Clean up volume processing nodes for a participant. */
  removeParticipantVolume(participantId: string): void {
    for (const key of this.pendingTracks.keys()) {
      if (key.startsWith(`${participantId}::`)) this.pendingTracks.delete(key);
    }
    const sources = this.volumeSources.get(participantId);
    if (sources) {
      sources.forEach((s) => s.disconnect());
      this.volumeSources.delete(participantId);
    }
    const gains = this.volumeGains.get(participantId);
    if (gains) {
      gains.forEach((g) => g.disconnect());
      this.volumeGains.delete(participantId);
    }
    const panners = this.spatialPanners.get(participantId);
    if (panners) {
      panners.forEach((p, trackName) => {
        p.disconnect();
        this.disposeBoost(participantId, trackName);
      });
      this.spatialPanners.delete(participantId);
    }
    const activators = this.activatorElements.get(participantId);
    if (activators) {
      activators.forEach((a) => {
        a.srcObject = null;
        a.remove();
      });
      this.activatorElements.delete(participantId);
    }
    this.directAudioElements.get(participantId)?.forEach((element) => {
      element.srcObject = null;
      element.remove();
    });
    this.directAudioElements.delete(participantId);
    for (const key of this.trackVolumeLevels.keys()) {
      if (key.startsWith(`${participantId}::`))
        this.trackVolumeLevels.delete(key);
    }
  }

  /** Clean up volume processing nodes for a single track of a participant. */
  removeTrackVolume(participantId: string, trackName: string): void {
    this.pendingTracks.delete(`${participantId}::${trackName}`);
    this.removeDirectAudioTrack(participantId, trackName);
    const sources = this.volumeSources.get(participantId);
    if (sources) {
      const source = sources.get(trackName);
      if (source) {
        source.disconnect();
        sources.delete(trackName);
      }
      if (sources.size === 0) this.volumeSources.delete(participantId);
    }
    const gains = this.volumeGains.get(participantId);
    if (gains) {
      const gain = gains.get(trackName);
      if (gain) {
        gain.disconnect();
        gains.delete(trackName);
      }
      if (gains.size === 0) this.volumeGains.delete(participantId);
    }
    const panners = this.spatialPanners.get(participantId);
    if (panners) {
      const panner = panners.get(trackName);
      if (panner) {
        panner.disconnect();
        panners.delete(trackName);
      }
      this.disposeBoost(participantId, trackName);
      if (panners.size === 0) this.spatialPanners.delete(participantId);
    }
    const activators = this.activatorElements.get(participantId);
    if (activators) {
      const activator = activators.get(trackName);
      if (activator) {
        activator.srcObject = null;
        activator.remove();
        activators.delete(trackName);
      }
      if (activators.size === 0) this.activatorElements.delete(participantId);
    }
  }

  /**
   * Set the master output volume (0.0 = silent, 1.0 = normal, 2.0 = max boost).
   * Maps from the UI's 0–200% slider via `level = slider / 100`.
   */
  setMasterVolume(level: number): void {
    const clamped = Math.max(0, Math.min(level, 2.0));
    this.masterVolumeLevel = clamped;
    for (const [participantId, elements] of this.directAudioElements) {
      for (const trackName of [...elements.keys()]) {
        const element = elements.get(trackName);
        if (!element) continue;
        if (this.getEffectiveDirectVolume(participantId, trackName) > 1) {
          this.promoteDirectTrackWhenRunning(participantId, trackName);
        } else {
          element.volume = this.getDirectElementVolume(
            participantId,
            trackName,
          );
        }
      }
    }
    if (!this.volumeContext && !this.masterGainNode) return;
    if (!this.volumeContext) {
      this.volumeContext =
        takePrewarmedAudioContext() ?? createPlaybackAudioContext();
    }
    const ctx = this.volumeContext;
    if (!this.masterGainNode) {
      this.masterGainNode = ctx.createGain();
      this.masterGainNode.connect(ctx.destination);
      // Re-connect limiter through master gain if it exists
      if (this.limiter) {
        this.limiter.disconnect();
        this.limiter.connect(this.masterGainNode);
      }
    }
    this.masterGainNode.gain.setTargetAtTime(
      clamped,
      ctx.currentTime || 0,
      0.05,
    );
  }

  /**
   * Set the output audio device. Uses AudioContext.setSinkId() where supported.
   * Falls back silently on browsers that don't support it.
   */
  async setOutputDevice(deviceId: string): Promise<void> {
    this.outputDeviceId = deviceId;
    for (const elements of this.directAudioElements.values()) {
      for (const element of elements.values()) {
        await this.applyDirectOutputDevice(element);
      }
    }

    if (!this.volumeContext) return;

    // Ensure context is running (may still be suspended if created outside
    // a user gesture and the prewarmed context wasn't available)
    this.volumeContext.resume().catch(() => {});

    const ctx = this.volumeContext as AudioContext & {
      setSinkId?: (id: string) => Promise<void>;
    };
    if (typeof ctx.setSinkId === "function") {
      try {
        const sinkId =
          deviceId === "default" || deviceId.startsWith("native:")
            ? ""
            : deviceId;
        await ctx.setSinkId(sinkId);
        audioLog.info(`Output device set to: ${deviceId}`);
      } catch (err) {
        audioLog.warn("Failed to set output device:", err);
      }
    } else {
      audioLog.warn("setSinkId not supported in this browser");
    }
  }

  private startFirstAudibleProbe(
    ctx: AudioContext,
    preLimiter: DynamicsCompressorNode,
    output: GainNode,
  ): void {
    if (this.outputAnalyser || this.callbacks.onFirstAudible === undefined) {
      return;
    }
    if (typeof ctx.createAnalyser !== "function") {
      audioLog.warn(
        "First-audible probe unavailable: createAnalyser unsupported",
      );
      return;
    }

    const probeStartedAt = Date.now();
    audioLog.info("First-audible probe starting", {
      contextState: ctx.state,
      masterGain: this.masterGainNode?.gain.value,
      probeStartedAt,
    });

    try {
      const analyser = ctx.createAnalyser();
      const preLimiterAnalyser = ctx.createAnalyser();
      analyser.fftSize = 256;
      preLimiterAnalyser.fftSize = 256;
      preLimiter.connect(preLimiterAnalyser);
      const silentProbeSink = ctx.createGain();
      silentProbeSink.gain.value = 0;
      preLimiterAnalyser.connect(silentProbeSink);
      silentProbeSink.connect(ctx.destination);
      output.disconnect();
      output.connect(analyser);
      analyser.connect(ctx.destination);
      this.preLimiterAnalyser = preLimiterAnalyser;
      this.preLimiterAnalyserData = new Uint8Array(preLimiterAnalyser.fftSize);
      this.outputAnalyser = analyser;
      this.outputAnalyserData = new Uint8Array(analyser.fftSize);
      this.firstAudibleAttempts = 0;
      const sample = () => {
        this.firstAudibleFrame = null;
        this.firstAudibleAttempts += 1;
        if (!this.outputAnalyser || !this.outputAnalyserData) return;
        this.outputAnalyser.getByteTimeDomainData(
          this.outputAnalyserData as Uint8Array<ArrayBuffer>,
        );
        if (this.preLimiterAnalyser && this.preLimiterAnalyserData) {
          this.preLimiterAnalyser.getByteTimeDomainData(
            this.preLimiterAnalyserData as Uint8Array<ArrayBuffer>,
          );
        }
        let outputSum = 0;
        let preLimiterSum = 0;
        for (
          let index = 0;
          index < this.outputAnalyserData.length;
          index += 1
        ) {
          const outputNormalized =
            (this.outputAnalyserData[index]! - 128) / 128;
          outputSum += outputNormalized * outputNormalized;
          if (this.preLimiterAnalyserData) {
            const preLimiterNormalized =
              (this.preLimiterAnalyserData[index]! - 128) / 128;
            preLimiterSum += preLimiterNormalized * preLimiterNormalized;
          }
        }
        const rms = Math.sqrt(outputSum / this.outputAnalyserData.length);
        const preLimiterRms = this.preLimiterAnalyserData
          ? Math.sqrt(preLimiterSum / this.preLimiterAnalyserData.length)
          : undefined;
        if (
          this.firstAudibleAttempts === 1 ||
          this.firstAudibleAttempts % 30 === 0
        ) {
          this.callbacks.onProbeSample?.({
            rms,
            preLimiterRms,
            contextState: ctx.state,
            elapsedMs: Date.now() - probeStartedAt,
          });
        }
        if (rms >= 0.003 && !this.firstAudibleReported) {
          this.firstAudibleReported = true;
          audioLog.info("First audible output detected", {
            rms,
            preLimiterRms,
            contextState: ctx.state,
            detectedAt: Date.now(),
            masterGain: this.masterGainNode?.gain.value,
          });
          this.callbacks.onFirstAudible?.({ rms });
          return;
        }
        if (!this.firstAudibleReported && ctx.state !== "closed") {
          if (this.firstAudibleAttempts >= 300) {
            audioLog.warn("First-audible probe timed out", {
              contextState: ctx.state,
              lastRms: rms,
              masterGain: this.masterGainNode?.gain.value,
            });
            return;
          }
          this.firstAudibleFrame =
            typeof requestAnimationFrame === "function"
              ? requestAnimationFrame(sample)
              : (setTimeout(sample, 16) as unknown as number);
        }
      };
      sample();
    } catch (error) {
      audioLog.warn("Unable to start first-audible probe", error);
    }
  }

  /** Dispose all audio resources. */
  dispose(): void {
    if (this.firstAudibleFrame !== null) {
      if (typeof cancelAnimationFrame === "function") {
        cancelAnimationFrame(this.firstAudibleFrame);
      } else {
        clearTimeout(this.firstAudibleFrame);
      }
      this.firstAudibleFrame = null;
    }
    if (this.preLimiterAnalyser) {
      this.preLimiterAnalyser.disconnect();
      this.preLimiterAnalyser = null;
      this.preLimiterAnalyserData = null;
    }
    if (this.outputAnalyser) {
      this.outputAnalyser.disconnect();
      this.outputAnalyser = null;
      this.outputAnalyserData = null;
    }
    this.firstAudibleReported = false;
    this.firstAudibleAttempts = 0;
    this.volumeGains.forEach((gains) => gains.forEach((g) => g.disconnect()));
    this.volumeGains.clear();
    this.spatialPanners.forEach((panners) =>
      panners.forEach((p) => p.disconnect()),
    );
    this.spatialPanners.clear();
    this.boostLingerTimers.forEach((timer) => clearTimeout(timer));
    this.boostLingerTimers.clear();
    this.boostLimiters.forEach((boost) => boost.dispose());
    this.boostLimiters.clear();
    this.volumeSources.forEach((sources) =>
      sources.forEach((s) => s.disconnect()),
    );
    this.volumeSources.clear();
    this.volumeLevels.clear();
    this.trackVolumeLevels.clear();
    this.trackPans.clear();
    this.activatorElements.forEach((acts) =>
      acts.forEach((a) => {
        a.srcObject = null;
        a.remove();
      }),
    );
    this.activatorElements.clear();
    this.directAudioElements.forEach((elements) =>
      elements.forEach((element) => {
        element.srcObject = null;
        element.remove();
      }),
    );
    this.directAudioElements.clear();
    this.pendingTracks.clear();
    this.stateChangeInstalled = false;
    if (this.limiter) {
      this.limiter.disconnect();
      this.limiter = null;
    }
    if (this.masterGainNode) {
      this.masterGainNode.disconnect();
      this.masterGainNode = null;
    }
    if (this.volumeContext) {
      this.volumeContext.onstatechange = null;
      this.volumeContext.close().catch(() => {});
      this.volumeContext = null;
    }
  }
}
