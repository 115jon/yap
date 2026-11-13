import { describe, expect, it } from "vitest";
import type { EmbedInfo } from "@/lib/types";
import { keptStoredEmbeds } from "../embed-removal";

const a = { id: "a", url: "https://a.example/", type: "link", fields: [] };
const b = { id: "b", url: "https://b.example/", type: "link", fields: [] };
const stored = [a, b] as EmbedInfo[];

describe("keptStoredEmbeds", () => {
  it("keeps nothing for an empty list", () => {
    expect(keptStoredEmbeds([], stored)).toEqual([]);
  });

  it("keeps the stored copies the request names, in stored order", () => {
    const forged = { ...b, thumbnail: { url: "https://tracker.example/p" } };
    expect(keptStoredEmbeds([forged, a], stored)).toEqual([a, b]);
  });

  it("refuses an embed the message does not have", () => {
    expect(
      keptStoredEmbeds([{ ...a, url: "https://other.example/" }], stored),
    ).toBeNull();
    expect(keptStoredEmbeds([{ id: "c", url: a.url }], stored)).toBeNull();
    expect(keptStoredEmbeds(["a"], stored)).toBeNull();
    expect(keptStoredEmbeds([null], stored)).toBeNull();
  });
});
