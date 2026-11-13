import { afterEach, describe, expect, it } from "vitest";
import {
  currentPublicOrigin,
  notePublicOrigin,
  publicOriginOf,
  resetPublicOriginForTests,
  withRequestOrigin,
} from "../origin";

function upgrade(url: string): Request {
  return new Request(url, { headers: { Upgrade: "websocket" } });
}

afterEach(() => resetPublicOriginForTests());

describe("publicOriginOf", () => {
  it("accepts client-facing origins", () => {
    expect(publicOriginOf("https://yap.115jon.com/api/x")).toBe(
      "https://yap.115jon.com",
    );
    expect(publicOriginOf("http://localhost:5198/a")).toBe(
      "http://localhost:5198",
    );
    expect(publicOriginOf("http://127.0.0.1:8787/a")).toBe(
      "http://127.0.0.1:8787",
    );
    expect(publicOriginOf("http://[::1]:8787/a")).toBe("http://[::1]:8787");
  });

  it("refuses the made-up hosts of internal stub calls", () => {
    expect(publicOriginOf("https://internal/api/channels/x/voice")).toBeNull();
    expect(publicOriginOf("https://do/x")).toBeNull();
    expect(publicOriginOf("ftp://yap.115jon.com/")).toBeNull();
    expect(publicOriginOf("not a url")).toBeNull();
  });
});

describe("notePublicOrigin", () => {
  it("remembers a public WebSocket upgrade's origin", () => {
    notePublicOrigin(upgrade("https://yap.example.com/api/channels/a/voice"));
    expect(currentPublicOrigin()).toBe("https://yap.example.com");
  });

  it("never remembers an internal host", () => {
    notePublicOrigin(upgrade("https://yap.example.com/api/channels/a/voice"));
    notePublicOrigin(upgrade("https://internal/api/channels/a/voice"));
    expect(currentPublicOrigin()).toBe("https://yap.example.com");
  });

  it("ignores requests that are not upgrades", () => {
    notePublicOrigin(new Request("https://other.example.com/"));
    expect(currentPublicOrigin()).toBe("https://yap.115jon.com");
  });

  it("prefers the current request's origin", () => {
    notePublicOrigin(upgrade("https://yap.example.com/api/channels/a/voice"));
    const seen = withRequestOrigin(
      new Request("https://preview.example.com/api/gifs"),
      () => currentPublicOrigin(),
    );
    expect(seen).toBe("https://preview.example.com");
  });
});
