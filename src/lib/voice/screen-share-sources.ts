// The web client's screen capture source for the @ralph/voice ScreenShare
// module: getDisplayMedia (the web has no native publisher). See
// packages/voice/SCREEN_SHARE.md.

import {
  ScreenShare,
  type PublicationReconciler,
  type ScreenCapture,
  type ScreenShareRequest,
  type ScreenShareSources,
} from "@ralph/voice";
import { clog } from "@/lib/console-logger";
import type {
  ScreenShareOptions,
  ScreenShareSourceState,
} from "@/lib/screen-share-types";
import type { SFUClient } from "@/lib/sfu-client";

const screenLog = clog("ScreenShare");

const SCREEN_QUALITY_MAP: Record<
  string,
  { width: number; height: number; bitrate: number }
> = {
  "720p": { width: 1280, height: 720, bitrate: 5_000_000 },
  "1080p": { width: 1920, height: 1080, bitrate: 8_000_000 },
  "1440p": { width: 2560, height: 1440, bitrate: 14_000_000 },
  "4k": { width: 3840, height: 2160, bitrate: 24_000_000 },
};

function getScreenQualitySettings(quality: string) {
  const fps = quality.endsWith("60") ? 60 : 30;
  const resKey = quality.replace(/\d+$/, "");
  const res = SCREEN_QUALITY_MAP[resKey];
  return { fps, res };
}

export function screenShareVideoConstraints(quality: string) {
  const { fps, res } = getScreenQualitySettings(quality);
  const browserVideoConstraints = {
    ...(res
      ? { width: { ideal: res.width }, height: { ideal: res.height } }
      : {}),
    frameRate: { ideal: fps, max: fps },
    cursor: "always",
  } as MediaTrackConstraints;

  return { fps, res, browserVideoConstraints };
}

function describeVideoTrack(track?: MediaStreamTrack) {
  if (!track) return null;
  const settings = track.getSettings();
  return {
    id: track.id,
    label: track.label,
    readyState: track.readyState,
    muted: track.muted,
    width: settings.width,
    height: settings.height,
    frameRate: settings.frameRate,
    displaySurface: (
      settings as MediaTrackSettings & { displaySurface?: string }
    ).displaySurface,
    deviceId: settings.deviceId,
  };
}

function describeAudioTrack(track?: MediaStreamTrack) {
  if (!track) return null;
  const settings = track.getSettings();
  return {
    id: track.id,
    label: track.label,
    readyState: track.readyState,
    muted: track.muted,
    deviceId: settings.deviceId,
    sampleRate: (settings as MediaTrackSettings & { sampleRate?: number })
      .sampleRate,
    channelCount: (settings as MediaTrackSettings & { channelCount?: number })
      .channelCount,
  };
}

export function logScreenShare(message: string, details?: unknown) {
  if (details === undefined) {
    screenLog.info(message);
    return;
  }
  try {
    screenLog.info(
      `${message} ${JSON.stringify(details, (_key, value) => {
        if (value instanceof Error) {
          return {
            name: value.name,
            message: value.message,
            stack: value.stack,
          };
        }
        return value;
      })}`,
    );
  } catch {
    screenLog.info(message, details);
  }
}

type QualityClient = Pick<SFUClient, "updateSenderEncoding">;

async function applyScreenTrackQuality(
  stream: MediaStream | null,
  quality: string,
  sfu: QualityClient | null,
  participantId: string | null,
) {
  const videoTrack = stream?.getVideoTracks()[0];
  if (!videoTrack) return;

  const { fps, res } = getScreenQualitySettings(quality);
  if (res) {
    logScreenShare("Applying screen quality", {
      quality,
      requested: {
        width: res.width,
        height: res.height,
        fps,
        bitrate: res.bitrate,
      },
      before: describeVideoTrack(videoTrack),
    });
    await videoTrack
      .applyConstraints({
        width: { ideal: res.width, max: res.width },
        height: { ideal: res.height, max: res.height },
        frameRate: { ideal: fps, max: fps },
      })
      .catch((err) => {
        screenLog.warn("Failed to apply video constraints:", err);
      });
    logScreenShare("Applied screen quality", {
      quality,
      after: describeVideoTrack(videoTrack),
      constraints: videoTrack.getConstraints?.(),
    });
  }

  if (!sfu || !participantId) return;

  await sfu
    .updateSenderEncoding(`screen-video-${participantId}`, {
      maxBitrate: res?.bitrate,
      maxFramerate: fps,
      scaleResolutionDownBy: 1,
    })
    .catch((err) => {
      screenLog.warn("Failed to update sender encoding:", err);
    });
  logScreenShare("Updated sender encoding", {
    trackName: `screen-video-${participantId}`,
    quality,
    maxBitrate: res?.bitrate,
    maxFramerate: fps,
  });
}

/** What the web asks the share module to capture: the picker's choice. */
export type WebScreenSource = ScreenShareSourceState &
  Pick<ScreenShareOptions, "pickerOpenedAt" | "pickerSelectionElapsedMs">;

export type WebScreenShareRequest = ScreenShareRequest<WebScreenSource>;

export interface WebScreenCapture extends ScreenCapture<MediaStreamTrack> {
  /** What the local tile shows: the browser capture itself. */
  stream: MediaStream | null;
}

export type WebScreenShare = ScreenShare<
  MediaStreamTrack,
  WebScreenSource,
  WebScreenCapture
>;

/** Builds a share request from the picker's options. */
export function webScreenShareRequest(
  options: ScreenShareOptions | undefined,
  defaults: { quality: string; audio: boolean },
): WebScreenShareRequest {
  const source: WebScreenSource = {
    sourceId: options?.sourceId ?? null,
    captureId: options?.captureId ?? null,
    sourceName: options?.sourceName ?? null,
    sourceKind: options?.sourceKind ?? null,
    sourceAppName: options?.sourceAppName ?? null,
    sourceIcon: options?.sourceIcon ?? null,
    pickerOpenedAt: options?.pickerOpenedAt,
    pickerSelectionElapsedMs: options?.pickerSelectionElapsedMs,
  };
  return {
    source,
    sourceKey: options?.sourceId ?? null,
    quality: options?.quality || defaults.quality,
    audio: options?.withAudio ?? defaults.audio,
    preferNative: false,
  };
}

/** The UI's view of a share's source (without picker timings). */
export function screenSourceState(
  source: WebScreenSource,
): ScreenShareSourceState {
  return {
    sourceId: source.sourceId ?? null,
    captureId: source.captureId ?? null,
    sourceName: source.sourceName ?? null,
    sourceKind: source.sourceKind ?? null,
    sourceAppName: source.sourceAppName ?? null,
    sourceIcon: source.sourceIcon ?? null,
  };
}

function stopStream(stream: MediaStream | null): void {
  stream?.getTracks().forEach((track) => {
    track.onended = null;
    track.stop();
  });
}

type ShareClient = Pick<SFUClient, "updateSenderEncoding">;

export interface WebScreenShareSourcesOptions {
  readonly sfu: ShareClient;
  readonly participantId: () => string;
  /** A capture's video ended by itself. */
  readonly onCaptureEnded: (capture: WebScreenCapture) => void;
}

export function createWebScreenShareSources({
  sfu,
  participantId,
  onCaptureEnded,
}: WebScreenShareSourcesOptions): ScreenShareSources<
  WebScreenSource,
  WebScreenCapture
> {
  return {
    native: null,
    browser: {
      async capture(request) {
        const { source, quality, audio } = request;
        const { browserVideoConstraints } =
          screenShareVideoConstraints(quality);
        const startedAt = performance.now();
        const elapsed = () => Math.round(performance.now() - startedAt);
        logScreenShare("Starting capture", {
          elapsedMs: 0,
          sourceId: source.sourceId ?? null,
          captureId: source.captureId ?? null,
          sourceKind: source.sourceKind ?? null,
          sourceName: source.sourceName ?? null,
          pickerSelectionElapsedMs: source.pickerSelectionElapsedMs ?? null,
          pickerToCaptureStartMs: source.pickerOpenedAt
            ? Math.round(startedAt - source.pickerOpenedAt)
            : null,
          quality,
          withAudio: audio,
          browserVideoConstraints,
        });

        let stream: MediaStream;
        const audioConstraints = audio
          ? {
              echoCancellation: false,
              noiseSuppression: false,
              autoGainControl: false,
            }
          : false;
        const displayMediaOptions: any = {
          video: browserVideoConstraints,
          audio: audioConstraints as any,
          monitorTypeSurfaces: "include",
          selfBrowserSurface: "exclude",
          surfaceSwitching: "include",
          systemAudio: audio ? "include" : "exclude",
          windowAudio: audio ? "window" : "exclude",
        };
        try {
          logScreenShare("Calling getDisplayMedia", {
            elapsedMs: elapsed(),
            options: displayMediaOptions,
          });
          stream =
            await navigator.mediaDevices.getDisplayMedia(displayMediaOptions);
        } catch (err: any) {
          if (!audio || err?.name === "NotAllowedError") throw err;
          logScreenShare(
            "getDisplayMedia with audio failed; retrying video-only",
            { elapsedMs: elapsed(), error: err },
          );
          stream = await navigator.mediaDevices.getDisplayMedia({
            video: browserVideoConstraints as any,
            audio: false,
          });
        }
        logScreenShare("getDisplayMedia returned", {
          elapsedMs: elapsed(),
          videoTracks: stream.getVideoTracks().map(describeVideoTrack),
          audioTracks: stream.getAudioTracks().map(describeAudioTrack),
        });

        const video = stream.getVideoTracks()[0];
        if (!video) {
          stopStream(stream);
          throw new Error("No screen share source was selected");
        }
        const audioTrack =
          stream
            .getAudioTracks()
            .find((track: MediaStreamTrack) => track.readyState === "live") ??
          null;
        const capture: WebScreenCapture = {
          path: "browser",
          video,
          audio: audioTrack,
          stream,
        };
        stream.getVideoTracks().forEach((track) => {
          track.contentHint = "detail";
          // The browser's "Stop sharing" (or the window closing) ends it.
          track.onended = () => onCaptureEnded(capture);
        });
        logScreenShare("Capture ready", {
          elapsedMs: elapsed(),
          videoTracks: stream.getVideoTracks().map(describeVideoTrack),
          videoContentHints: stream
            .getVideoTracks()
            .map((track) => track.contentHint),
          audioTracks: stream.getAudioTracks().map(describeAudioTrack),
          screenHasAudio: audioTrack !== null,
        });
        return capture;
      },
      applyQuality(capture, quality) {
        return applyScreenTrackQuality(
          capture.stream,
          quality,
          sfu,
          participantId() || null,
        );
      },
    },
    // A getDisplayMedia restart keeps the old capture until the picker
    // returns, so cancelling keeps the share.
    releaseBeforeRestart: () => false,
    isLive: (capture) => capture.video?.readyState === "live",
    release(capture) {
      stopStream(capture.stream);
      for (const track of [capture.video, capture.audio]) {
        if (!track) continue;
        track.onended = null;
        track.stop();
      }
      capture.stream = null;
    },
  };
}

/** One client's share module, publishing through that client's reconciler. */
export function createWebScreenShare(
  options: Omit<WebScreenShareSourcesOptions, "onCaptureEnded"> & {
    readonly publication: PublicationReconciler<MediaStreamTrack>;
  },
): WebScreenShare {
  let share: WebScreenShare | null = null;
  const sources = createWebScreenShareSources({
    ...options,
    onCaptureEnded: (capture) => share?.captureEnded(capture),
  });
  share = new ScreenShare({
    publication: options.publication,
    sources,
    onError: (step, error) => {
      screenLog.warn(`Screen share step failed: ${step}`, error);
    },
  });
  return share;
}
