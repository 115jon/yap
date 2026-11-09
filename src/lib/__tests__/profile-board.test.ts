import { describe, expect, it } from "vitest";

import type { Entitlement } from "@/lib/entitlements";
import {
  boardBytes,
  boardForViewer,
  cleanText,
  cleanUrl,
  MAX_BOARD_BYTES,
  MAX_WIDGETS,
  packRack,
  parseActivitySettings,
  parseBoard,
  parseWishlist,
  TEXT_LIMITS,
  textLength,
  type ProfileBoard,
} from "@/lib/profile-board";

const NOW = Date.parse("2026-11-09T12:00:00Z");
const loud: Entitlement = {
  tier: "loud",
  source: "stripe",
  started_at: "2026-10-01T00:00:00Z",
  expires_at: "2099-01-01T00:00:00Z",
};
const lapsed: Entitlement = { ...loud, expires_at: "2026-11-01T00:00:00Z" };

function widget(type: string, extra: Record<string, unknown> = {}) {
  return { id: `w${Math.random().toString(36).slice(2, 10)}`, type, ...extra };
}

describe("cleanText", () => {
  it("strips controls and bidi overrides, collapses spaces, trims", () => {
    expect(cleanText("  hi‮there\n\tfriend\u0000  ", 40)).toBe("hithere friend");
  });

  it("cuts by grapheme clusters, keeping emoji whole", () => {
    const family = "👨‍👩‍👧";
    expect(textLength(family)).toBe(1);
    expect(cleanText(`${family}${family}${family}`, 2)).toBe(
      `${family}${family}`,
    );
  });

  it("treats anything but a string as empty", () => {
    expect(cleanText(42, 10)).toBe("");
    expect(cleanText({ toString: () => "x" }, 10)).toBe("");
  });
});

describe("cleanUrl", () => {
  it("keeps https links and adds the scheme to bare hosts", () => {
    expect(cleanUrl("https://kiwi.page/art")).toBe("https://kiwi.page/art");
    expect(cleanUrl("steamcommunity.com/id/kiwi")).toBe(
      "https://steamcommunity.com/id/kiwi",
    );
  });

  it.each([
    "javascript:alert(1)",
    "http://kiwi.page",
    "data:text/html,hi",
    "https://user:pass@kiwi.page",
    "https://127.0.0.1/",
    "https://[::1]/",
    "https://localhost/",
    "https://kiwi.page:8443/",
    `https://kiwi.page/${"a".repeat(300)}`,
  ])("refuses %s", (value) => {
    expect(cleanUrl(value)).toBeNull();
  });
});

describe("parseBoard", () => {
  it("drops unknown types and reports them", () => {
    const { board, dropped } = parseBoard({
      widgets: [widget("mood"), widget("hologram")],
    });
    expect(board.widgets.map((w) => w.type)).toEqual(["mood"]);
    expect(dropped).toEqual([
      expect.objectContaining({ type: "hologram", reason: "unknown_type" }),
    ]);
  });

  it("caps the board at 12 widgets and each type at its max", () => {
    const many = [
      ...Array.from({ length: 3 }, () => widget("links")),
      ...Array.from({ length: 2 }, () => widget("mood")),
      ...Array.from({ length: 12 }, () => widget("photos")),
    ];
    const { board, dropped } = parseBoard({ widgets: many });
    expect(board.widgets.filter((w) => w.type === "links")).toHaveLength(2);
    expect(board.widgets.filter((w) => w.type === "mood")).toHaveLength(1);
    expect(board.widgets.filter((w) => w.type === "photos")).toHaveLength(2);
    expect(dropped.map((d) => d.reason)).toContain("type_limit");

    const all = parseBoard({
      widgets: [
        "now_playing",
        "top_games",
        "voice_hours",
        "listening",
        "badges",
        "servers",
        "quote",
        "guestbook",
        "mood",
        "photos",
        "links",
        "countdown",
        "custom",
      ].map((type) => widget(type)),
    });
    expect(all.board.widgets).toHaveLength(MAX_WIDGETS);
    expect(all.dropped).toEqual([
      expect.objectContaining({ type: "custom", reason: "board_limit" }),
    ]);
  });

  it("snaps a size a type can't take to its default", () => {
    const { board } = parseBoard({
      widgets: [widget("mood", { size: "L" }), widget("links", { size: "Q" })],
    });
    expect(board.widgets.map((w) => w.size)).toEqual(["S", "S"]);
  });

  it("cleans every config field and drops bad links and assets", () => {
    const { board } = parseBoard({
      widgets: [
        widget("links", {
          cfg: {
            links: [
              { label: "Steam‮", url: "steamcommunity.com/id/kiwi" },
              { label: "bad", url: "javascript:alert(1)" },
            ],
            extra: "dropped",
          },
        }),
        widget("custom", {
          cfg: {
            legend: "x".repeat(60),
            title: "Commissions open",
            asset: "../../etc/passwd",
            lines: ["a", "b", "c", "d", "e"],
          },
        }),
        widget("photos", {
          cfg: {
            photos: [
              { asset: "abcdefghijkl0123.webp", alt: "Lisbon" },
              { asset: "https://evil.example/x.png" },
            ],
          },
        }),
      ],
    });
    const [links, custom, photos] = board.widgets;
    expect(links.cfg).toEqual({
      links: [{ label: "Steam", url: "https://steamcommunity.com/id/kiwi" }],
    });
    expect(custom.cfg).toMatchObject({
      legend: "x".repeat(TEXT_LIMITS.legend),
      asset: null,
    });
    expect((custom.cfg as { lines: string[] }).lines).toHaveLength(4);
    expect((photos.cfg as { photos: unknown[] }).photos).toEqual([
      { asset: "abcdefghijkl0123.webp", alt: "Lisbon" },
    ]);
  });

  it("drops duplicate ids and reads stored JSON strings", () => {
    const { board, dropped } = parseBoard(
      JSON.stringify({
        widgets: [
          { id: "same-id", type: "mood" },
          { id: "same-id", type: "links" },
        ],
      }),
    );
    expect(board.widgets).toHaveLength(1);
    expect(dropped[0]?.reason).toBe("duplicate_id");
  });

  it("never throws on garbage", () => {
    for (const value of [null, 1, "nope", [], { widgets: "x" }]) {
      expect(parseBoard(value).board).toEqual({ v: 1, widgets: [] });
    }
  });

  it("keeps a full board under the stored size ceiling", () => {
    const long = "w".repeat(200);
    const { board } = parseBoard({
      widgets: [
        widget("custom", {
          cfg: { legend: long, title: long, lines: [long, long, long, long] },
        }),
        widget("custom", {
          cfg: { legend: long, title: long, lines: [long, long, long, long] },
        }),
        widget("links", {
          cfg: {
            links: Array.from({ length: 5 }, () => ({
              label: long,
              url: `https://kiwi.page/${"p".repeat(150)}`,
            })),
          },
        }),
      ],
    });
    expect(boardBytes(board)).toBeLessThan(MAX_BOARD_BYTES);
  });
});

describe("boardForViewer", () => {
  const board = parseBoard({
    widgets: [
      { id: "custom-1", type: "custom", cfg: { title: "Mine" } },
      {
        id: "mood-1",
        type: "mood",
        cfg: { text: "hi", clears_at: "2026-11-01T00:00:00Z" },
      },
    ],
  }).board as ProfileBoard;

  it("hides Custom from others without the perk, and drafts it for the owner", () => {
    const other = boardForViewer(board, {
      isOwner: false,
      ownerEntitlement: null,
      now: NOW,
    });
    expect(other.board.widgets.map((w) => w.id)).toEqual(["mood-1"]);
    expect(other.drafts).toEqual([]);

    const owner = boardForViewer(board, {
      isOwner: true,
      ownerEntitlement: null,
      now: NOW,
    });
    expect(owner.board.widgets.map((w) => w.id)).toEqual([
      "custom-1",
      "mood-1",
    ]);
    expect(owner.drafts).toEqual(["custom-1"]);
  });

  it("shows Custom while Loud and hides it again once the plan lapses", () => {
    expect(
      boardForViewer(board, {
        isOwner: false,
        ownerEntitlement: loud,
        now: NOW,
      }).board.widgets.map((w) => w.id),
    ).toContain("custom-1");
    expect(
      boardForViewer(board, {
        isOwner: false,
        ownerEntitlement: lapsed,
        now: NOW,
      }).board.widgets.map((w) => w.id),
    ).not.toContain("custom-1");
  });

  it("empties a mood past its clear time for others only", () => {
    const other = boardForViewer(board, {
      isOwner: false,
      ownerEntitlement: loud,
      now: NOW,
    }).board.widgets.find((w) => w.id === "mood-1");
    expect(other?.cfg).toMatchObject({ text: "" });
    const owner = boardForViewer(board, {
      isOwner: true,
      ownerEntitlement: loud,
      now: NOW,
    }).board.widgets.find((w) => w.id === "mood-1");
    expect(owner?.cfg).toMatchObject({ text: "hi" });
  });
});

describe("packRack", () => {
  it("places sizes on two columns without gaps (dense)", () => {
    // M takes the left column for two rows; the S beside it fills the
    // right; L goes below both; the last S fills its row's left half.
    const { cells, rows } = packRack(["M", "S", "S", "L", "S"]);
    expect(cells).toEqual([
      { col: 0, row: 0, cols: 1, rows: 2 },
      { col: 1, row: 0, cols: 1, rows: 1 },
      { col: 1, row: 1, cols: 1, rows: 1 },
      { col: 0, row: 2, cols: 2, rows: 1 },
      { col: 0, row: 3, cols: 1, rows: 1 },
    ]);
    expect(rows).toBe(4);
  });

  it("back-fills a hole an earlier L left", () => {
    const { cells } = packRack(["S", "L", "S"]);
    expect(cells[1]).toEqual({ col: 0, row: 1, cols: 2, rows: 1 });
    expect(cells[2]).toEqual({ col: 1, row: 0, cols: 1, rows: 1 });
  });

  it("is empty for an empty board", () => {
    expect(packRack([])).toEqual({ cells: [], rows: 0 });
  });
});

describe("parseWishlist", () => {
  it("keeps shop items and hints, cleaned and deduplicated", () => {
    const items = parseWishlist([
      { kind: "shop", sku: "pixel-bloom", name: "Pixel bloom" },
      { kind: "shop", sku: "pixel-bloom", name: "again" },
      { kind: "shop", sku: "../bad", name: "x" },
      { kind: "hint", text: "  a  good   mic  " },
      { kind: "hint", text: "   " },
      { kind: "gift-card" },
    ]);
    expect(items.map((item) => item.kind)).toEqual(["shop", "hint"]);
    expect(items[1]).toMatchObject({ text: "a good mic" });
  });

  it("caps the list", () => {
    const items = parseWishlist(
      Array.from({ length: 40 }, (_, index) => ({
        kind: "hint",
        text: `hint ${index}`,
      })),
    );
    expect(items).toHaveLength(24);
  });
});

describe("parseActivitySettings", () => {
  it("falls back to 30 days and ignores unknown kinds", () => {
    expect(
      parseActivitySettings({
        retention_days: 12,
        hidden_kinds: ["game", "x"],
      }),
    ).toEqual({ retention_days: 30, hidden_kinds: ["game"] });
    expect(parseActivitySettings({ retention_days: 90 })).toEqual({
      retention_days: 90,
      hidden_kinds: [],
    });
  });
});
