import { describe, expect, it } from "vitest";
import fc from "fast-check";
import {
  resolveScreenVideoSubscription,
  resolveStreamThumbnails,
} from "@/hooks/useVoiceChannel";

describe("resolveScreenVideoSubscription", () => {
  it("pulls screen video only while the share is watched or on stage", () => {
    fc.assert(
      fc.property(fc.boolean(), fc.boolean(), (isWatched, isFocused) => {
        expect(resolveScreenVideoSubscription({ isWatched, isFocused })).toBe(
          isWatched || isFocused,
        );
      }),
      { numRuns: 20 },
    );
  });

  it("shows an unwatched share's broadcast still, keeping local and captured ones", () => {
    const captured = { me: "local-capture", bob: "old-capture" };
    const still = (name: string) => `data:image/jpeg;base64,${name}`;
    const merged = resolveStreamThumbnails(
      captured,
      [
        { user_id: "me", stream_preview_url: still("mine") },
        { user_id: "bob", stream_preview_url: still("bob") },
        { user_id: "ann", stream_preview_url: still("ann") },
        { user_id: "cy", stream_preview_url: null },
      ],
      "me",
    );
    expect(merged).toEqual({
      me: "local-capture",
      bob: still("bob"),
      ann: still("ann"),
    });
    expect(captured).toEqual({ me: "local-capture", bob: "old-capture" });
    // Nothing to add: the same object, so memoized consumers keep theirs.
    expect(resolveStreamThumbnails(captured, [], "me")).toBe(captured);
  });

  it("ignores a broadcast still that is not an inline image", () => {
    const captured = { bob: "old-capture" };
    const merged = resolveStreamThumbnails(
      captured,
      [
        { user_id: "bob", stream_preview_url: "https://evil.example/x.png" },
        { user_id: "ann", stream_preview_url: "file://evil.example/x.png" },
      ],
      "me",
    );
    expect(merged).toBe(captured);
  });

  it("does not pull an unwatched share's video", () => {
    expect(
      resolveScreenVideoSubscription({ isWatched: false, isFocused: false }),
    ).toBe(false);
  });
});
