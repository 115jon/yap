export interface MediaDeviceInfo_Custom {
  deviceId: string;
  groupId?: string;
  label: string;
  kind: MediaDeviceKind;
}

export interface MediaDeviceSnapshot {
  audioInputs: MediaDeviceInfo_Custom[];
  audioOutputs: MediaDeviceInfo_Custom[];
  videoInputs: MediaDeviceInfo_Custom[];
  rawDevices: MediaDeviceInfo[];
}

/**
 * The browser's device list. Discovery stays passive: actual capture belongs
 * to an explicit voice join, camera preview, or microphone test.
 */
export async function enumerateMediaDevicesWithRetry() {
  return navigator.mediaDevices.enumerateDevices();
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

  return { rawDevices, audioInputs, audioOutputs, videoInputs };
}
