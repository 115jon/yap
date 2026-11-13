import { describe, expect, it } from "vitest";
import {
  classifyMediaUrl,
  cleanUrlInput,
  isOwnZoneHostname,
} from "../media-url-class";

function hostOf(input: string): string | null {
  const classified = classifyMediaUrl(input);
  return classified.kind === "absolute" ? classified.url.host : null;
}

describe("cleanUrlInput", () => {
  it("drops what the URL parser drops", () => {
    expect(cleanUrlInput("/\t/evil.example/a")).toBe("//evil.example/a");
    expect(cleanUrlInput("/\n\r/evil.example/a")).toBe("//evil.example/a");
    expect(cleanUrlInput("\u0000\u001f https://a.example/ \u0007")).toBe(
      "https://a.example/",
    );
    // A space inside stays: the parser encodes it.
    expect(cleanUrlInput("/a b.png")).toBe("/a b.png");
  });
});

describe("classifyMediaUrl", () => {
  it("reads a path of ours", () => {
    expect(classifyMediaUrl("/api/attachments/a.png")).toEqual({
      kind: "path",
      value: "/api/attachments/a.png",
    });
    expect(classifyMediaUrl(" /og-image.png\n")).toEqual({
      kind: "path",
      value: "/og-image.png",
    });
  });

  it.each([
    "//evil.example/a.png",
    "/\\evil.example/a.png",
    "\\\\evil.example/a.png",
    "\\/evil.example/a.png",
    "/\t/evil.example/a.png",
    "/\n/evil.example/a.png",
    "\t\t//evil.example/a.png",
    "https://evil.example/a.png",
    "HTTPS://EVIL.example/a.png",
    "https:\\\\evil.example/a.png",
    "https:///evil.example/a.png",
    "h\tttps://evil.example/a.png",
    "\u0000https://evil.example/a.png",
  ])("reads %j as another host", (input) => {
    expect(hostOf(input)).toBe("evil.example");
  });

  it.each([
    "https:evil.example/a.png",
    "http:/evil.example/a.png",
    "data:image/png;base64,AA",
    "javascript:alert(1)",
    "blob:https://yap.example/1",
    "ftp://evil.example/a",
    "a.png",
    "",
    "https://",
  ])("reads %j as neither", (input) => {
    expect(classifyMediaUrl(input).kind).toBe("other");
  });
});

describe("isOwnZoneHostname", () => {
  it.each([
    "115jon.com",
    "yap.115jon.com",
    "YAP.115JON.COM",
    "yap.115jon.com.",
    "a.b.115jon.com",
    "115jon.site",
    "meet.115jon.site.",
  ])("owns %s", (name) => {
    expect(isOwnZoneHostname(name)).toBe(true);
  });

  it.each([
    "x115jon.com",
    "115jon.com.evil.example",
    "115jon.co",
    "example.com",
  ])("does not own %s", (name) => {
    expect(isOwnZoneHostname(name)).toBe(false);
  });
});
