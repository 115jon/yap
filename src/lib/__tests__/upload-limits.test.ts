import { describe, expect, it } from "vitest";

import { CAMERA_BACKGROUND_UPLOAD_LIMIT_BYTES } from "../camera-background-validation";
import { MAX_GIF_UPLOAD_BYTES } from "../gif-picker";
import { MAX_PROFILE_BANNER_SIZE } from "../profile-asset-validation";
import {
  formatUploadLimit,
  formatUploadOverage,
  formatUploadSize,
  isOverUploadLimit,
  MB,
  parseServerLimitBytes,
  partitionBySize,
  serverLimitLabel,
  tooLargeMessage,
  UPLOAD_LIMITS,
  type UploadKind,
} from "../upload-limits";
import { MAX_VOICE_STATUS_MEDIA_UPLOAD_BYTES } from "../voice-status-media";

describe("UPLOAD_LIMITS", () => {
  it("matches what the server enforces, in 1024 * 1024 megabytes", () => {
    expect(MB).toBe(1024 * 1024);
    expect(UPLOAD_LIMITS).toEqual({
      avatar: 8 * MB,
      banner: 8 * MB,
      attachment: 25 * MB,
      gif: 25 * MB,
      vibe: 25 * MB,
      cameraBackground: 25 * MB,
      soundboard: 50 * MB,
    });
  });

  it("is the one constant the older per-kind names point at", () => {
    expect(MAX_GIF_UPLOAD_BYTES).toBe(UPLOAD_LIMITS.gif);
    expect(MAX_VOICE_STATUS_MEDIA_UPLOAD_BYTES).toBe(UPLOAD_LIMITS.vibe);
    expect(CAMERA_BACKGROUND_UPLOAD_LIMIT_BYTES).toBe(
      UPLOAD_LIMITS.cameraBackground,
    );
    expect(MAX_PROFILE_BANNER_SIZE).toBe(UPLOAD_LIMITS.banner);
  });
});

describe("size formatting", () => {
  it("prints sizes in MB with one decimal", () => {
    expect(formatUploadSize(Math.round(31.2 * MB))).toBe("31.2 MB");
    expect(formatUploadSize(20.4 * MB)).toBe("20.4 MB");
    expect(formatUploadSize(63 * MB)).toBe("63.0 MB");
    expect(formatUploadSize(0)).toBe("0.0 MB");
  });

  it("prints limits with no decimal", () => {
    expect(formatUploadLimit(8 * MB)).toBe("8 MB");
    expect(formatUploadLimit(25 * MB)).toBe("25 MB");
    expect(formatUploadLimit(50 * MB)).toBe("50 MB");
    // A limit that is not whole keeps its decimal rather than lie.
    expect(formatUploadLimit(7.5 * MB)).toBe("7.5 MB");
  });

  it("rounds the overage up so a file just over never reads 0.0 MB over", () => {
    expect(formatUploadOverage(Math.round(31.2 * MB), 25 * MB)).toBe("6.2 MB");
    expect(formatUploadOverage(25 * MB + 1, 25 * MB)).toBe("0.1 MB");
  });
});

describe("limit checks", () => {
  it("lets a file at the limit through and stops one byte over", () => {
    expect(isOverUploadLimit(25 * MB, UPLOAD_LIMITS.attachment)).toBe(false);
    expect(isOverUploadLimit(25 * MB + 1, UPLOAD_LIMITS.attachment)).toBe(true);
  });

  it("splits a drop into the files that fit and the ones that don't", () => {
    const a = { name: "a", size: 1 };
    const b = { name: "b", size: 26 * MB };
    const c = { name: "c", size: 25 * MB };
    expect(partitionBySize([a, b, c], UPLOAD_LIMITS.attachment)).toEqual({
      fits: [a, c],
      tooBig: [b],
    });
  });
});

describe("the server's 413 message", () => {
  it("says the limit the way the routes always have", () => {
    expect(serverLimitLabel(8 * MB)).toBe("8MB");
    expect(tooLargeMessage("Avatar", UPLOAD_LIMITS.avatar)).toBe(
      "Avatar too large (max 8MB)",
    );
    expect(tooLargeMessage("File", UPLOAD_LIMITS.soundboard)).toBe(
      "File too large (max 50MB)",
    );
  });

  it("reads the number back", () => {
    expect(parseServerLimitBytes("Avatar too large (max 8MB)")).toBe(8 * MB);
    expect(parseServerLimitBytes("File too large (max 25 MB)")).toBe(25 * MB);
    expect(parseServerLimitBytes("too large (MAX 7.5mb)")).toBe(7.5 * MB);
    for (const kind of Object.keys(UPLOAD_LIMITS) as UploadKind[]) {
      expect(
        parseServerLimitBytes(tooLargeMessage("X", UPLOAD_LIMITS[kind])),
      ).toBe(UPLOAD_LIMITS[kind]);
    }
  });

  it("returns null when no limit is named", () => {
    expect(parseServerLimitBytes("The file is too large")).toBeNull();
    expect(parseServerLimitBytes("max 0MB")).toBeNull();
    expect(parseServerLimitBytes(undefined)).toBeNull();
    expect(parseServerLimitBytes(413)).toBeNull();
  });
});
