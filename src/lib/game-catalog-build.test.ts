import { describe, expect, it } from "vitest";

import { buildCatalog, trimApp } from "../../scripts/lib/game-catalog.mjs";

const HASH = "a60bb76ba4d4acafbd4cb9aad6e61739";

const overwatch = {
  aliases: [],
  cover_image_hash: "843a3b07639f068fdacf40b9c3808c46",
  executables: [
    { is_launcher: false, name: "overwatch.exe", os: "win32" },
    { is_launcher: false, name: "overwatch", os: "darwin" },
  ],
  hook: true,
  icon_hash: HASH,
  id: "356875221078245376",
  name: "Overwatch",
  themes: ["Action"],
  third_party_skus: [
    { distributor: "xbox", id: "BRKX1W0QDF6L" },
    { distributor: "steam", id: "2357570" },
    { distributor: "battlenet", id: "Pro" },
  ],
};

describe("trimApp", () => {
  it("keeps only the catalog fields and win32 executables", () => {
    expect(trimApp(overwatch)).toEqual({
      id: "356875221078245376",
      name: "Overwatch",
      icon_hash: HASH,
      executables: [{ name: "overwatch.exe" }],
      steam_app_ids: ["2357570"],
    });
  });

  it("carries aliases, launcher flags and arguments only when set", () => {
    const app = trimApp({
      id: "1",
      name: "  Game  ",
      aliases: ["Game: Remastered", "", "  "],
      executables: [
        { os: "win32", name: "launcher.exe", is_launcher: true },
        { os: "win32", name: "game.exe", is_launcher: false, arguments: "-x" },
        { os: "win32", name: "plain.exe", arguments: "" },
        { os: "linux", name: "game" },
      ],
    });
    expect(app).toEqual({
      id: "1",
      name: "Game",
      aliases: ["Game: Remastered"],
      executables: [
        { name: "launcher.exe", is_launcher: true },
        { name: "game.exe", arguments: "-x" },
        { name: "plain.exe" },
      ],
    });
  });

  it("omits a missing or malformed icon hash", () => {
    const base = {
      id: "2",
      name: "Game",
      executables: [{ os: "win32", name: "g.exe" }],
    };
    expect(trimApp({ ...base })).not.toHaveProperty("icon_hash");
    expect(trimApp({ ...base, icon_hash: null })).not.toHaveProperty(
      "icon_hash",
    );
    expect(trimApp({ ...base, icon_hash: "../x" })).not.toHaveProperty(
      "icon_hash",
    );
    expect(
      trimApp({ ...base, icon_hash: HASH.toUpperCase() }),
    ).not.toHaveProperty("icon_hash");
  });

  it("collects every numeric Steam app id once and ignores other stores", () => {
    const app = trimApp({
      id: "3",
      name: "Game",
      executables: [{ os: "win32", name: "g.exe" }],
      third_party_skus: [
        { distributor: "steam", id: "10" },
        { distributor: "steam", id: "10" },
        { distributor: "steam", id: "20" },
        { distributor: "steam", id: "not-a-number" },
        { distributor: "gog", id: "30" },
        { distributor: "epic", id: "40" },
      ],
    });
    expect(app?.steam_app_ids).toEqual(["10", "20"]);
  });

  it("drops apps without a win32 executable or a usable id or name", () => {
    expect(
      trimApp({
        id: "4",
        name: "Mac",
        executables: [{ os: "darwin", name: "m" }],
      }),
    ).toBeNull();
    expect(trimApp({ id: "5", name: "None", executables: [] })).toBeNull();
    expect(trimApp({ id: "6", name: "Missing" })).toBeNull();
    expect(
      trimApp({
        id: "x",
        name: "Bad",
        executables: [{ os: "win32", name: "b" }],
      }),
    ).toBeNull();
    expect(
      trimApp({
        id: "7",
        name: " ",
        executables: [{ os: "win32", name: "b" }],
      }),
    ).toBeNull();
    expect(trimApp(null)).toBeNull();
    expect(trimApp("app")).toBeNull();
  });
});

describe("buildCatalog", () => {
  const generatedAt = new Date("2026-11-08T12:00:00.000Z");
  const win = (id: string, name: string) => ({
    id,
    name,
    executables: [{ os: "win32", name: `${name.toLowerCase()}.exe` }],
  });

  it("sorts by numeric id, drops undetectable apps and reports counts", () => {
    const { catalog, stats } = buildCatalog(
      [
        win("300", "C"),
        {
          id: "250",
          name: "Mac only",
          executables: [{ os: "darwin", name: "m" }],
        },
        win("20", "B"),
        win("3000000000000000000", "D"),
        { name: "no id" },
        win("100", "A"),
      ],
      { generatedAt },
    );
    expect(catalog.apps.map((app: { id: string }) => app.id)).toEqual([
      "20",
      "100",
      "300",
      "3000000000000000000",
    ]);
    expect(catalog).toMatchObject({
      version: 1,
      generated_at: "2026-11-08T12:00:00.000Z",
      source: "discord-detectable",
      count: 4,
    });
    expect(stats).toEqual({ input: 6, kept: 4, dropped: 1, invalid: 1 });
  });

  it("keeps the first of duplicate ids", () => {
    const { catalog } = buildCatalog([win("1", "First"), win("1", "Second")], {
      generatedAt,
    });
    expect(catalog.apps.map((app: { name: string }) => app.name)).toEqual([
      "First",
    ]);
  });

  it("is deterministic: input order does not change the apps or the etag", () => {
    const apps = [win("5", "E"), win("2", "B"), win("9", "I")];
    const a = buildCatalog(apps, { generatedAt });
    const b = buildCatalog([...apps].reverse(), { generatedAt });
    expect(JSON.stringify(a.catalog)).toBe(JSON.stringify(b.catalog));
    expect(a.catalog.etag).toMatch(/^[a-f0-9]{16}$/);
  });

  it("changes the etag when the apps change", () => {
    const a = buildCatalog([win("1", "A")], { generatedAt });
    const b = buildCatalog([win("1", "A"), win("2", "B")], { generatedAt });
    expect(a.catalog.etag).not.toBe(b.catalog.etag);
  });

  it("rejects input that is not an array", () => {
    expect(() => buildCatalog({ apps: [] })).toThrow(/array/);
  });
});
