import { describe, expect, it } from "vitest";

import { viaMediaProxy } from "./media-proxy";

const API = "https://yap.example";

describe("viaMediaProxy", () => {
  it("routes a third-party image through the API's proxy", () => {
    expect(viaMediaProxy("https://cdn.example.com/a.png?x=1", API)).toBe(
      `${API}/api/proxy-media?url=https%3A%2F%2Fcdn.example.com%2Fa.png%3Fx%3D1`,
    );
    expect(viaMediaProxy("http://plain.example/a.gif", API)).toBe(
      `${API}/api/proxy-media?url=http%3A%2F%2Fplain.example%2Fa.gif`,
    );
  });

  it("leaves the API's own media and server-built proxy URLs alone", () => {
    for (const uri of [
      `${API}/api/attachments/c/a/b.png`,
      `${API}/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png&sig=s`,
    ]) {
      expect(viaMediaProxy(uri, API)).toBe(uri);
    }
  });

  it("sends a proxy-looking path on another host to our API, not there", () => {
    expect(
      viaMediaProxy(
        "https://evil.example/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png&sig=s",
        API,
      ),
    ).toBe(
      `${API}/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png&sig=s`,
    );
    expect(
      viaMediaProxy("https://evil.example/api/proxy-media?id=1", API),
    ).toBe(`${API}/api/proxy-media?id=1`);
  });

  it("leaves local files and data URIs alone", () => {
    for (const uri of [
      "file:///data/x.jpg",
      "content://media/1",
      "data:image/png;base64,AA",
    ]) {
      expect(viaMediaProxy(uri, API)).toBe(uri);
    }
  });

  it("proxies another host hidden behind what URL libraries ignore", () => {
    for (const uri of [
      " https://evil.example/a.png",
      "\thttps://evil.example/a.png",
      "https://evil.example/a.png\n",
      "ht\ttps://evil.example/a.png",
      "\u0000https://evil.example/a.png",
      "HTTPS://evil.example/a.png",
      "https:evil.example/a.png",
      "https:/evil.example/a.png",
      "https:\\\\evil.example/a.png",
      "//evil.example/a.png",
      "/\t/evil.example/a.png",
      "\\\\evil.example/a.png",
    ]) {
      expect(viaMediaProxy(uri, API), JSON.stringify(uri)).toBe(
        `${API}/api/proxy-media?url=https%3A%2F%2Fevil.example%2Fa.png`,
      );
    }
  });

  it("keeps the API's own URLs, however they are written", () => {
    expect(viaMediaProxy(` ${API}/api/attachments/a.png\n`, API)).toBe(
      `${API}/api/attachments/a.png`,
    );
  });

  it("leaves values that name no web host alone", () => {
    for (const uri of ["https:", "https://", "https:?x", "a.png", ""]) {
      expect(viaMediaProxy(uri, API), JSON.stringify(uri)).toBe(uri);
    }
  });
});
