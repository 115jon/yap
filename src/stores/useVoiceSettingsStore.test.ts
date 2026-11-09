import { beforeEach, describe, expect, it } from "vitest";
import {
  normalizePeerSettings,
  useVoiceSettingsStore,
} from "./useVoiceSettingsStore";

describe("useVoiceSettingsStore peer volume settings", () => {
  beforeEach(() => {
    useVoiceSettingsStore.setState({
      currentUser: null,
      userSettings: {},
      _cache: {},
    });
  });

  it("preserves explicit peer stream volume independently from voice volume", () => {
    const store = useVoiceSettingsStore.getState();
    store.setCurrentUser("viewer");
    store.setPeerVolume("user-2", 55);
    store.setPeerStreamVolume("user-2", 25);

    const peer = useVoiceSettingsStore.getState().getSettings("viewer")
      .peerSettings["user-2"];
    expect(peer.volume).toBe(55);
    expect(peer.streamVolume).toBe(25);
  });

  it("backfills stream volume from voice volume for older stored peer settings", () => {
    expect(normalizePeerSettings({ volume: 42 } as any).streamVolume).toBe(42);
  });

  it("keeps per-user volumes in 0..400 % and older values as stored", () => {
    const store = useVoiceSettingsStore.getState();
    store.setCurrentUser("viewer");
    store.setPeerVolume("user-2", 950);
    store.setPeerStreamVolume("user-2", -20);
    let peer = useVoiceSettingsStore.getState().getSettings("viewer")
      .peerSettings["user-2"];
    expect(peer.volume).toBe(400);
    expect(peer.streamVolume).toBe(0);

    store.setPeerVolume("user-2", 250);
    store.setPeerStreamVolume("user-2", 400);
    peer = useVoiceSettingsStore.getState().getSettings("viewer").peerSettings[
      "user-2"
    ];
    expect(peer.volume).toBe(250);
    expect(peer.streamVolume).toBe(400);

    // Stored by a release that capped at 200 %: unchanged.
    expect(normalizePeerSettings({ volume: 180, streamVolume: 200 })).toEqual(
      expect.objectContaining({ volume: 180, streamVolume: 200 }),
    );
    expect(normalizePeerSettings({ volume: 1000, streamVolume: -5 })).toEqual(
      expect.objectContaining({ volume: 400, streamVolume: 0 }),
    );
    expect(normalizePeerSettings({ volume: Number.NaN }).volume).toBe(100);
    expect(normalizePeerSettings({ volume: 900 }).streamVolume).toBe(400);
  });

  it("defaults camera capture and background settings for existing users", () => {
    useVoiceSettingsStore.setState({
      currentUser: "viewer",
      userSettings: {
        viewer: {
          inputDeviceId: "default",
          outputDeviceId: "default",
          videoDeviceId: "default",
        } as any,
      },
      _cache: {},
    });

    expect(
      useVoiceSettingsStore.getState().getSettings("viewer"),
    ).toMatchObject({
      cameraQuality: "720p30",
      cameraBackground: { type: "none" },
      customCameraBackgrounds: [],
    });
  });

  it("persists screen share defaults for the current user", () => {
    const store = useVoiceSettingsStore.getState();
    store.setCurrentUser("viewer");

    store.setScreenShareDefaults({ quality: "1080p60", withAudio: false });

    expect(
      useVoiceSettingsStore.getState().getSettings("viewer"),
    ).toMatchObject({
      screenShareQuality: "1080p60",
      screenShareWithAudio: false,
    });
  });

  it("applies explicit Studio settings without profile inference", () => {
    const store = useVoiceSettingsStore.getState();
    store.setCurrentUser("viewer");
    store.setInputProfile("studio");

    expect(
      useVoiceSettingsStore.getState().getSettings("viewer"),
    ).toMatchObject({
      inputProfile: "studio",
      streamHighFidelity: true,
      noiseSuppression: false,
      noiseReductionEnabled: false,
      echoCancellation: false,
      autoSensitivity: false,
      spatialAudioEnabled: false,
    });

    store.updateVoiceSettings((settings) => ({
      ...settings,
      autoSensitivity: true,
    }));

    expect(
      useVoiceSettingsStore.getState().getSettings("viewer").inputProfile,
    ).toBe("custom");
  });
});
