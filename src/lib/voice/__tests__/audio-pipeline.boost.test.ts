// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  AudioPipeline,
  BOOST_LIMITER_LINGER_MS,
  BOOST_TRIM,
  createBoostLimiter,
  disposePrewarmedAudioContext,
} from "../audio-pipeline";

vi.mock("@/lib/sounds", () => ({
  resumeSoundContext: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@/lib/voice/capture-audio-context", () => ({
  prewarmCaptureAudioContext: vi.fn(),
}));

class TestMediaStream {
  constructor(private readonly tracks: MediaStreamTrack[]) {}
  getTracks() {
    return this.tracks;
  }
  getAudioTracks() {
    return this.tracks.filter((track) => track.kind === "audio");
  }
}

type Node = {
  kind: string;
  connect: ReturnType<typeof vi.fn>;
  disconnect: ReturnType<typeof vi.fn>;
};
type Param = { value: number; setTargetAtTime: ReturnType<typeof vi.fn> };
type Compressor = Node & {
  threshold: Param;
  knee: Param;
  ratio: Param;
  attack: Param;
  release: Param;
};
type Gain = Node & { gain: Param };

function param(value = 0): Param {
  return { value, setTargetAtTime: vi.fn() };
}

function createAudioContext() {
  const compressors: Compressor[] = [];
  const gains: Gain[] = [];
  const panners: Node[] = [];
  const node = (kind: string): Node => ({
    kind,
    connect: vi.fn(),
    disconnect: vi.fn(),
  });
  const context = {
    state: "running",
    currentTime: 0,
    destination: {},
    onstatechange: null,
    resume: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    createMediaStreamSource: vi.fn(() => node("source")),
    createDynamicsCompressor: vi.fn(() => {
      const compressor = {
        ...node("compressor"),
        threshold: param(),
        knee: param(),
        ratio: param(),
        attack: param(),
        release: param(),
      };
      compressors.push(compressor);
      return compressor;
    }),
    createGain: vi.fn(() => {
      const gain = { ...node("gain"), gain: param(1) };
      gains.push(gain);
      return gain;
    }),
    createStereoPanner: vi.fn(() => {
      const panner = { ...node("panner"), pan: param() };
      panners.push(panner);
      return panner;
    }),
  } as unknown as AudioContext;
  return { context, compressors, gains, panners };
}

const TRACK = "mic-audio-p";

function setUp() {
  const audio = createAudioContext();
  vi.stubGlobal(
    "AudioContext",
    vi.fn(function () {
      return audio.context;
    }),
  );
  const pipeline = new AudioPipeline({ onAudioResumed: vi.fn() });
  return { ...audio, pipeline };
}

function boostCount(pipeline: AudioPipeline): number {
  return (pipeline as unknown as { boostLimiters: Map<string, unknown> })
    .boostLimiters.size;
}

function lastTarget(node: Node): object | undefined {
  return node.connect.mock.calls.at(-1)?.[0] as object | undefined;
}

describe("AudioPipeline boost (per-user volume up to 400 %)", () => {
  beforeEach(() => {
    disposePrewarmedAudioContext();
    vi.useFakeTimers();
    vi.stubGlobal("MediaStream", TestMediaStream);
    HTMLMediaElement.prototype.play = vi.fn().mockResolvedValue(undefined);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("builds the limiter at the native parameters, unity under the ceiling", () => {
    const { context, compressors, gains } = createAudioContext();
    const boost = createBoostLimiter(context);
    const [compressor] = compressors;
    expect(boost.input).toBe(compressor);
    expect(compressor?.threshold.value).toBe(-3);
    expect(compressor?.knee.value).toBe(0);
    expect(compressor?.ratio.value).toBe(20);
    expect(compressor?.attack.value).toBe(0.0015);
    expect(compressor?.release.value).toBe(0.06);
    // The trim cancels the compressor's make-up gain: (1 / g0dB) ^ 0.6 for
    // the -2.85 dB a 0 dBFS input comes out at.
    expect(gains[0]?.gain.value).toBeCloseTo(BOOST_TRIM, 6);
    expect(BOOST_TRIM).toBeCloseTo(Math.pow(10, -2.85 / 20) ** 0.6, 6);
    expect(compressor?.connect).toHaveBeenCalledWith(boost.output);
  });

  it("clamps participant and track volume to 0..4 and applies up to 4x", () => {
    const { pipeline, gains } = setUp();
    pipeline.applyVolumeToTrack(
      "p",
      { kind: "audio", readyState: "live" } as MediaStreamTrack,
      TRACK,
    );
    // gains[0] is the master gain, gains[1] the track's.
    const trackGain = gains[1]!;

    pipeline.setParticipantVolume("p", 9);
    expect(pipeline.getParticipantVolume("p")).toBe(4);
    expect(trackGain.gain.setTargetAtTime).toHaveBeenLastCalledWith(4, 0, 0.1);
    pipeline.setParticipantVolume("p", -1);
    expect(pipeline.getParticipantVolume("p")).toBe(0);

    pipeline.setTrackVolume("p", TRACK, 3.5);
    expect(trackGain.gain.setTargetAtTime).toHaveBeenLastCalledWith(
      3.5,
      0,
      0.1,
    );
    pipeline.setTrackVolume("p", TRACK, 12);
    expect(trackGain.gain.setTargetAtTime).toHaveBeenLastCalledWith(4, 0, 0.1);
    pipeline.dispose();
  });

  it("inserts a limiter only above 1x and removes it after coming back to 1x", () => {
    const { pipeline, compressors, panners } = setUp();
    pipeline.applyVolumeToTrack(
      "p",
      { kind: "audio", readyState: "live" } as MediaStreamTrack,
      TRACK,
    );
    const mix = compressors[0]!;
    const panner = panners[0]!;
    // At 100 %: the plain path, panner straight into the mix limiter.
    expect(compressors).toHaveLength(1);
    expect(lastTarget(panner)).toBe(mix);
    expect(boostCount(pipeline)).toBe(0);

    pipeline.setParticipantVolume("p", 1);
    expect(compressors).toHaveLength(1);

    pipeline.setParticipantVolume("p", 2.5);
    expect(compressors).toHaveLength(2);
    const boost = compressors[1]!;
    expect(lastTarget(panner)).toBe(boost);
    expect(boostCount(pipeline)).toBe(1);
    // Moving within the boost keeps the same limiter.
    pipeline.setParticipantVolume("p", 4);
    expect(compressors).toHaveLength(2);

    // Back to 100 %: the limiter lingers while the gain glides down...
    pipeline.setParticipantVolume("p", 1);
    expect(lastTarget(panner)).toBe(boost);
    vi.advanceTimersByTime(BOOST_LIMITER_LINGER_MS - 1);
    expect(boostCount(pipeline)).toBe(1);
    // ...then the track returns to the plain path.
    vi.advanceTimersByTime(1);
    expect(lastTarget(panner)).toBe(mix);
    expect(boost.disconnect).toHaveBeenCalled();
    expect(boostCount(pipeline)).toBe(0);
    pipeline.dispose();
  });

  it("keeps the limiter when the boost comes back before it lets go", () => {
    const { pipeline, compressors, panners } = setUp();
    pipeline.applyVolumeToTrack(
      "p",
      { kind: "audio", readyState: "live" } as MediaStreamTrack,
      TRACK,
    );
    pipeline.setTrackVolume("p", TRACK, 3);
    const boost = compressors[1]!;
    pipeline.setTrackVolume("p", TRACK, 0.5);
    vi.advanceTimersByTime(BOOST_LIMITER_LINGER_MS / 2);
    pipeline.setTrackVolume("p", TRACK, 2);
    vi.advanceTimersByTime(BOOST_LIMITER_LINGER_MS * 2);
    expect(compressors).toHaveLength(2);
    expect(lastTarget(panners[0]!)).toBe(boost);
    expect(boostCount(pipeline)).toBe(1);
    pipeline.dispose();
    expect(boostCount(pipeline)).toBe(0);
  });

  it("starts a track that joins boosted behind its limiter, and frees it on removal", () => {
    const { pipeline, compressors, panners } = setUp();
    pipeline.setParticipantVolume("p", 3);
    pipeline.applyVolumeToTrack(
      "p",
      { kind: "audio", readyState: "live" } as MediaStreamTrack,
      TRACK,
    );
    expect(compressors).toHaveLength(2);
    const boost = compressors[1]!;
    expect(lastTarget(panners[0]!)).toBe(boost);
    expect(boost.connect).toHaveBeenCalled();

    pipeline.removeTrackVolume("p", TRACK);
    expect(boost.disconnect).toHaveBeenCalled();
    expect(boostCount(pipeline)).toBe(0);
    pipeline.dispose();
  });

  it("uses the worklet limiter once its module loads, upgrading a stand-in", async () => {
    const audio = createAudioContext();
    let finishLoad: () => void = () => {};
    const addModule = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finishLoad = resolve;
        }),
    );
    Object.assign(audio.context, { audioWorklet: { addModule } });
    const worklets: Array<
      Node & { port: { postMessage: ReturnType<typeof vi.fn> } }
    > = [];
    vi.stubGlobal(
      "AudioWorkletNode",
      vi.fn(function (_ctx: unknown, name: string) {
        expect(name).toBe("rm-boost-limiter");
        const worklet = {
          kind: "worklet",
          connect: vi.fn(),
          disconnect: vi.fn(),
          port: { postMessage: vi.fn() },
        };
        worklets.push(worklet);
        return worklet;
      }),
    );
    vi.stubGlobal(
      "AudioContext",
      vi.fn(function () {
        return audio.context;
      }),
    );
    const pipeline = new AudioPipeline({ onAudioResumed: vi.fn() });
    pipeline.applyVolumeToTrack(
      "p",
      { kind: "audio", readyState: "live" } as MediaStreamTrack,
      TRACK,
    );
    expect(addModule).toHaveBeenCalledWith(
      expect.stringContaining("boost-limiter.worklet"),
    );
    // Boosted before the module is ready: the compressor stands in...
    pipeline.setParticipantVolume("p", 3);
    const standIn = audio.compressors[1]!;
    expect(lastTarget(audio.panners[0]!)).toBe(standIn);

    // ...and is swapped for the worklet when it loads.
    finishLoad();
    await vi.waitFor(() => expect(worklets).toHaveLength(1));
    const worklet = worklets[0]!;
    expect(lastTarget(audio.panners[0]!)).toBe(worklet);
    expect(worklet.connect).toHaveBeenCalledWith(audio.compressors[0]);
    expect(standIn.disconnect).toHaveBeenCalled();

    // Back at 100 %, the worklet is stopped and the plain path restored.
    pipeline.setParticipantVolume("p", 1);
    vi.advanceTimersByTime(BOOST_LIMITER_LINGER_MS);
    expect(worklet.port.postMessage).toHaveBeenCalledWith({ type: "stop" });
    expect(lastTarget(audio.panners[0]!)).toBe(audio.compressors[0]);

    // A later boost goes straight to the worklet.
    pipeline.setParticipantVolume("p", 2);
    expect(worklets).toHaveLength(2);
    expect(audio.compressors).toHaveLength(2);
    pipeline.dispose();
    expect(worklets[1]!.port.postMessage).toHaveBeenCalledWith({
      type: "stop",
    });
  });

  it("promotes a direct sink to the graph, behind the limiter, at 400 %", () => {
    const { pipeline, compressors, gains } = setUp();
    pipeline.attachRemoteAudioTrack(
      "p",
      { kind: "audio", readyState: "live" } as MediaStreamTrack,
      TRACK,
    );
    expect(compressors).toHaveLength(0);
    pipeline.setParticipantVolume("p", 4);
    // The <audio> element caps at 1.0, so 4x moves to a GainNode.
    expect(gains[1]?.gain.value).toBe(4);
    expect(compressors).toHaveLength(2);
    expect(boostCount(pipeline)).toBe(1);
    pipeline.dispose();
  });
});
