import { clog } from "@/lib/console-logger";

const mediaLog = clog("MediaDevices");

export interface MediaDeviceInfo_Custom {
  deviceId: string;
  groupId?: string;
  label: string;
  kind: MediaDeviceKind;
  nativeDeviceId?: string;
  isNative?: boolean;
  isDefault?: boolean;
}

export interface NativeDevice {
  device_id: string;
  label: string;
  kind: "audioinput" | "audiooutput" | "videoinput";
  is_default: boolean;
}

export interface MediaDeviceSnapshot {
  audioInputs: MediaDeviceInfo_Custom[];
  audioOutputs: MediaDeviceInfo_Custom[];
  videoInputs: MediaDeviceInfo_Custom[];
  nativeAudioDevices: NativeDevice[];
  nativeVideoDevices: NativeDevice[];
  rawDevices: MediaDeviceInfo[];
}

/**
 * The browser's device list. Discovery stays passive: actual capture belongs
 * to an explicit voice join, camera preview, or microphone test.
 */
export async function enumerateMediaDevicesWithRetry() {
  return navigator.mediaDevices.enumerateDevices();
}

function hasBrowserGeneratedLabel(
  device: MediaDeviceInfo_Custom,
  index: number,
) {
  const fallbackName =
    device.kind === "audioinput"
      ? `Microphone ${index + 1}`
      : `Speaker ${index + 1}`;
  return device.label.trim().length === 0 || device.label === fallbackName;
}

function normalizedDeviceLabel(label: string) {
  return label.trim().toLowerCase();
}

export function mergeNativeAudioLabels(
  browserDevices: MediaDeviceInfo_Custom[],
  nativeDevices: NativeDevice[],
  kind: "audioinput" | "audiooutput",
): MediaDeviceInfo_Custom[] {
  const matchingNativeDevices = nativeDevices
    .filter((device) => device.kind === kind)
    .sort((a, b) => Number(b.is_default) - Number(a.is_default));

  if (matchingNativeDevices.length === 0) return browserDevices;

  const defaultNativeDevice = matchingNativeDevices.find(
    (device) => device.is_default,
  );
  const nonDefaultNativeDevices = matchingNativeDevices.filter(
    (device) => !device.is_default,
  );
  const usedNativeIds = new Set<string>();
  let nonDefaultIndex = 0;

  const mergedBrowserDevices = browserDevices.map((device, index) => {
    if (device.deviceId === "default") {
      if (defaultNativeDevice) {
        usedNativeIds.add(defaultNativeDevice.device_id);
      }
      return {
        ...device,
        label: defaultNativeDevice?.label || device.label,
        isDefault: true,
        isNative: Boolean(defaultNativeDevice),
      };
    }

    if (!hasBrowserGeneratedLabel(device, index)) {
      const nativeMatch = matchingNativeDevices.find(
        (nativeDevice) =>
          normalizedDeviceLabel(nativeDevice.label) ===
          normalizedDeviceLabel(device.label),
      );
      if (nativeMatch) {
        usedNativeIds.add(nativeMatch.device_id);
        return {
          ...device,
          nativeDeviceId: nativeMatch.device_id,
          isNative: true,
          isDefault: nativeMatch.is_default,
        };
      }
      return device;
    }

    const nativeDevice =
      nonDefaultNativeDevices[nonDefaultIndex] ??
      matchingNativeDevices[nonDefaultIndex];
    nonDefaultIndex++;

    if (!nativeDevice?.label) return device;

    return {
      ...device,
      label: nativeDevice.label,
      nativeDeviceId: nativeDevice.device_id,
      isNative: true,
      isDefault: nativeDevice.is_default,
    };
  });

  const hiddenNativeCount = matchingNativeDevices.length - usedNativeIds.size;
  if (hiddenNativeCount > 0) {
    mediaLog.debug(
      "Native devices not exposed by CEF/WebRTC were hidden from selectable list",
      {
        kind,
        hiddenNativeCount,
        browserDeviceCount: browserDevices.length,
        nativeDeviceCount: matchingNativeDevices.length,
      },
    );
  }

  return mergedBrowserDevices;
}

export function mergeNativeVideoLabels(
  browserDevices: MediaDeviceInfo_Custom[],
  nativeDevices: NativeDevice[],
): MediaDeviceInfo_Custom[] {
  const matchingNativeDevices = nativeDevices.filter(
    (device) => device.kind === "videoinput",
  );

  if (matchingNativeDevices.length === 0) return browserDevices;

  const usedNativeIds = new Set<string>();
  let cameraIndex = 0;

  const mergedBrowserDevices = browserDevices.map((device, index) => {
    // If we have a useful browser-provided label, try to match by name
    if (
      device.label.trim().length > 0 &&
      device.label !== `Camera ${index + 1}`
    ) {
      const nativeMatch = matchingNativeDevices.find(
        (nativeDevice) =>
          normalizedDeviceLabel(nativeDevice.label) ===
          normalizedDeviceLabel(device.label),
      );
      if (nativeMatch) {
        usedNativeIds.add(nativeMatch.device_id);
        return {
          ...device,
          nativeDeviceId: nativeMatch.device_id,
          isNative: true,
        };
      }
      return device;
    }

    // Otherwise, assign sequentially from the native list
    const nativeDevice = matchingNativeDevices[cameraIndex];
    if (nativeDevice) {
      cameraIndex++;
      usedNativeIds.add(nativeDevice.device_id);
      return {
        ...device,
        label: nativeDevice.label,
        nativeDeviceId: nativeDevice.device_id,
        isNative: true,
      };
    }

    return device;
  });

  const hiddenNativeCount = matchingNativeDevices.length - usedNativeIds.size;
  if (hiddenNativeCount > 0) {
    mediaLog.debug(
      "Native video devices not exposed by CEF/WebRTC were hidden from selectable list",
      {
        hiddenNativeCount,
        browserDeviceCount: browserDevices.length,
        nativeDeviceCount: matchingNativeDevices.length,
      },
    );
  }

  return mergedBrowserDevices;
}

export async function getMediaDeviceSnapshot(): Promise<MediaDeviceSnapshot> {
  const rawDevices = await enumerateMediaDevicesWithRetry();
  const audioInputs = rawDevices
    .filter((d) => d.kind === "audioinput")
    .map((d, i) => ({
      deviceId: d.deviceId,
      groupId: d.groupId,
      label:
        d.label ||
        (d.deviceId === "default"
          ? "Default microphone"
          : `Microphone ${i + 1}`),
      kind: d.kind,
    }));
  const audioOutputs = rawDevices
    .filter((d) => d.kind === "audiooutput")
    .map((d, i) => ({
      deviceId: d.deviceId,
      groupId: d.groupId,
      label:
        d.label ||
        (d.deviceId === "default" ? "Default speaker" : `Speaker ${i + 1}`),
      kind: d.kind,
    }));
  const videoInputs = rawDevices
    .filter((d) => d.kind === "videoinput")
    .map((d, i) => ({
      deviceId: d.deviceId,
      groupId: d.groupId,
      label: d.label || `Camera ${i + 1}`,
      kind: d.kind,
    }));

  // The web has no OS device list to merge labels from.
  const nativeAudioDevices: NativeDevice[] = [];
  const nativeVideoDevices: NativeDevice[] = [];

  return {
    rawDevices,
    nativeAudioDevices,
    nativeVideoDevices,
    audioInputs: mergeNativeAudioLabels(
      audioInputs,
      nativeAudioDevices,
      "audioinput",
    ),
    audioOutputs: mergeNativeAudioLabels(
      audioOutputs,
      nativeAudioDevices,
      "audiooutput",
    ),
    videoInputs: mergeNativeVideoLabels(videoInputs, nativeVideoDevices),
  };
}
