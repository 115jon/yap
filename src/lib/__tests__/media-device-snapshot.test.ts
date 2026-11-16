import { afterEach, describe, expect, it, vi } from "vitest";
import { enumerateMediaDevicesWithRetry } from "../media-device-snapshot";

describe("media-device-snapshot", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("enumerates devices without acquiring a temporary microphone stream", async () => {
    const enumerateDevices = vi.fn().mockResolvedValue([
      {
        deviceId: "mic-1",
        groupId: "group-1",
        label: "Microphone",
        kind: "audioinput",
      },
    ]);
    const getUserMedia = vi.fn();

    vi.stubGlobal("navigator", {
      mediaDevices: { enumerateDevices, getUserMedia },
    });

    await expect(enumerateMediaDevicesWithRetry()).resolves.toHaveLength(1);
    expect(enumerateDevices).toHaveBeenCalled();
    expect(getUserMedia).not.toHaveBeenCalled();
  });
});
