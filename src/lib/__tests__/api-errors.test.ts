import { describe, expect, it } from "vitest";
import {
  describeApiError,
  isTooLargeError,
  serverUploadLimit,
} from "../api-errors";

function httpError(status: number, message = "D1_ERROR: no such table") {
  return Object.assign(new Error(message), { status });
}

describe("describeApiError", () => {
  it("never shows the server's message", () => {
    for (const status of [400, 401, 403, 404, 409, 413, 429, 500, 503]) {
      const copy = describeApiError(httpError(status), "save the role");
      expect(copy).not.toContain("D1_ERROR");
      expect(copy).toContain("save the role");
    }
  });

  it("asks to sign in again on 401", () => {
    expect(
      describeApiError(httpError(401, "Authentication required"), "load bans"),
    ).toBe("Your session has ended. Sign in again to load bans.");
  });

  it("names a missing permission on 403", () => {
    expect(describeApiError(httpError(403), "unban members")).toBe(
      "You don't have permission to unban members.",
    );
  });

  it("points at the connection when there is no status", () => {
    expect(describeApiError(new TypeError("Failed to fetch"), "load")).toBe(
      "Unable to load. Check your connection and try again.",
    );
    expect(describeApiError("boom", "load")).toBe(
      "Unable to load. Check your connection and try again.",
    );
    expect(describeApiError(null, "load")).toBe(
      "Unable to load. Check your connection and try again.",
    );
  });

  it("separates client, rate-limit and server failures", () => {
    expect(describeApiError(httpError(409), "create the invite")).toBe(
      "Unable to create the invite. Check the details and try again.",
    );
    expect(describeApiError(httpError(429), "send")).toBe(
      "Unable to send right now. Wait a moment and try again.",
    );
    expect(describeApiError(httpError(502), "send")).toBe(
      "Unable to send. Try again in a moment.",
    );
    expect(describeApiError(httpError(404), "revoke the invite")).toBe(
      "Unable to revoke the invite. It may have been deleted.",
    );
  });
});

describe("413: the server's own limit survives", () => {
  it("says the limit the server named", () => {
    expect(
      describeApiError(
        httpError(413, "Avatar too large (max 8MB)"),
        "upload your avatar",
      ),
    ).toBe("Unable to upload your avatar. The server says the limit is 8 MB.");
    expect(
      describeApiError(httpError(413, "File too large (max 50MB)"), "send"),
    ).toBe("Unable to send. The server says the limit is 50 MB.");
  });

  it("falls back to the client's limit when the body names none", () => {
    expect(
      describeApiError(httpError(413, "Payload Too Large"), "upload the icon", {
        uploadLimitBytes: 8 * 1024 * 1024,
      }),
    ).toBe("Unable to upload the icon. The server says the limit is 8 MB.");
  });

  it("never says 'The file is too large'", () => {
    for (const message of ["Avatar too large (max 8MB)", "", "D1_ERROR"]) {
      const copy = describeApiError(httpError(413, message), "upload it");
      expect(copy).not.toContain("The file is too large");
    }
    expect(describeApiError(httpError(413), "upload the icon")).toBe(
      "Unable to upload the icon. It's over the server's size limit.",
    );
  });

  it("reads the limit only from a 413", () => {
    expect(
      serverUploadLimit(httpError(413, "Banner too large (max 8MB)")),
    ).toBe(8 * 1024 * 1024);
    expect(serverUploadLimit(httpError(400, "max 8MB"))).toBeNull();
    expect(serverUploadLimit(httpError(413, "too large"))).toBeNull();
    expect(serverUploadLimit(null)).toBeNull();
    expect(isTooLargeError(httpError(413))).toBe(true);
    expect(isTooLargeError(httpError(400))).toBe(false);
  });
});
