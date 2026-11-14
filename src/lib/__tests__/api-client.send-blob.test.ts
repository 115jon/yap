import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/desktop-auth", () => ({
  getDesktopAuthHandoffToken: vi.fn(() => null),
  getStoredKovaAuthSessionToken: vi.fn(() => "test-token"),
  waitForDesktopToken: vi.fn(async () => "test-token"),
}));

vi.mock("@/lib/app-session-recovery", () => ({
  getAppSessionState: () => "signed-out",
  recoverAppSession: vi.fn(async () => null),
}));

vi.mock("@/lib/platform", () => ({
  apiUrl: (path: string) => `https://meet.test${path}`,
}));

vi.mock("@/lib/kova-auth-config", () => ({
  KOVA_AUTH_PUBLISHABLE_KEY: null,
}));

vi.mock("@/lib/console-logger", () => ({
  clog: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

import { apiPost, apiSendBlob } from "@/lib/api-client";
import { isRetryableUploadError } from "@/lib/multipart-upload";

/** Just enough XMLHttpRequest: the test drives its events by hand. */
class FakeXhr {
  static last: FakeXhr | null = null;
  status = 0;
  responseText = "";
  responseType = "";
  headers: Record<string, string> = {};
  responseHeaders: Record<string, string> = {};
  upload: {
    onprogress?: (e: { loaded: number }) => void;
    onload?: () => void;
  } = {};
  onload?: () => void;
  onerror?: () => void;
  ontimeout?: () => void;
  onabort?: () => void;
  onprogress?: () => void;
  aborted = false;
  constructor() {
    FakeXhr.last = this;
  }
  open() {}
  setRequestHeader(name: string, value: string) {
    this.headers[name] = value;
  }
  getResponseHeader(name: string) {
    return this.responseHeaders[name] ?? null;
  }
  send() {}
  abort() {
    this.aborted = true;
    this.onabort?.();
  }
  respond(status: number, body: unknown, headers: Record<string, string> = {}) {
    this.status = status;
    this.responseText = JSON.stringify(body);
    this.responseHeaders = headers;
    this.onload?.();
  }
}

/** Let the awaited token lookup run so the request is actually sent. */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("apiSendBlob", () => {
  beforeEach(() => {
    FakeXhr.last = null;
    vi.stubGlobal("XMLHttpRequest", FakeXhr);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("sends with the bearer and resolves the JSON body", async () => {
    const pending = apiSendBlob("/api/x", new Blob(["abc"]));
    await flush();
    const xhr = FakeXhr.last!;
    expect(xhr.headers.Authorization).toBe("Bearer test-token");
    xhr.respond(200, { part_number: 1, size: 3 });
    await expect(pending).resolves.toEqual({ part_number: 1, size: 3 });
  });

  it("fails a stalled part as a retryable network error", async () => {
    const pending = apiSendBlob("/api/x", new Blob(["abc"]), {
      stallTimeoutMs: 40,
    });
    const caught = pending.catch((error: unknown) => error);
    await flush();
    const xhr = FakeXhr.last!;
    const error = await caught;
    expect(xhr.aborted).toBe(true);
    expect(error).toBeInstanceOf(TypeError);
    expect(isRetryableUploadError(error)).toBe(true);
  });

  it("keeps a slow part alive while bytes keep moving", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const pending = apiSendBlob("/api/x", new Blob(["abc"]), {
      stallTimeoutMs: 1000,
    });
    await vi.advanceTimersByTimeAsync(0);
    const xhr = FakeXhr.last!;
    for (let i = 1; i <= 5; i += 1) {
      await vi.advanceTimersByTimeAsync(800);
      xhr.upload.onprogress?.({ loaded: i });
    }
    expect(xhr.aborted).toBe(false);
    xhr.respond(200, { ok: true });
    await expect(pending).resolves.toEqual({ ok: true });
  });

  it("a cancel is an AbortError, not a stall", async () => {
    const controller = new AbortController();
    const pending = apiSendBlob("/api/x", new Blob(["abc"]), {
      signal: controller.signal,
    });
    await flush();
    controller.abort();
    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
  });

  it("carries status, code and Retry-After on an error", async () => {
    const pending = apiSendBlob("/api/x", new Blob(["abc"]));
    await flush();
    FakeXhr.last!.respond(
      429,
      { error: "Rate limit exceeded", code: "RATE" },
      { "Retry-After": "30" },
    );
    await expect(pending).rejects.toMatchObject({
      status: 429,
      code: "RATE",
      retryAfterMs: 30_000,
    });
  });
});

describe("JSON helpers and Retry-After", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("puts a 429's Retry-After on the thrown error", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () =>
        Response.json(
          { error: "Too many uploads in progress.", code: "TOO_MANY_UPLOADS" },
          { status: 429, headers: { "Retry-After": "30" } },
        ),
      ),
    );
    await expect(apiPost("/api/x", {})).rejects.toMatchObject({
      status: 429,
      code: "TOO_MANY_UPLOADS",
      retryAfterMs: 30_000,
    });
  });
});
