import { createRequire } from "node:module";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ServiceError } from "@/lib/service-error";

// node:sqlite ships in Node 22+, but the pinned @types/node (20) does not
// describe it, so it is loaded untyped and given the slice used here.
type SqliteStatement = {
  get: (...args: unknown[]) => unknown;
  all: (...args: unknown[]) => unknown[];
  run: (...args: unknown[]) => unknown;
};
type SqliteDatabase = {
  exec: (sql: string) => void;
  prepare: (sql: string) => SqliteStatement;
};
const { DatabaseSync } = createRequire(import.meta.url)("node:sqlite") as {
  DatabaseSync: new (path: string) => SqliteDatabase;
};

const mocks = vi.hoisted(() => ({
  requireAuth: vi.fn(),
  getDB: vi.fn(),
  getMe: vi.fn(),
  getCurrentUser: vi.fn(),
}));

vi.mock("@/lib/api-helpers", () => ({
  apiError: (error: string, status = 400, code?: string) =>
    Response.json({ error, code }, { status }),
  apiSuccess: (data: unknown) => Response.json(data),
  getDB: mocks.getDB,
  requireAuth: mocks.requireAuth,
}));
vi.mock("@/lib/ensure-user-profile-schema", () => ({
  ensureUserProfileSchema: vi.fn(),
}));
vi.mock("@/services/user.service", () => ({ getMe: mocks.getMe }));
vi.mock("@/lib/kova-auth-server", () => ({
  getCurrentUser: mocks.getCurrentUser,
}));

import { GET } from "../users/me";

// A D1-shaped adapter over an in-memory SQLite database, so the sync queries
// run as written.
function d1(sqlite: SqliteDatabase) {
  const statement = (sql: string, args: unknown[] = []) => ({
    bind: (...next: unknown[]) => statement(sql, next),
    first: async () => sqlite.prepare(sql).get(...args) ?? null,
    all: async () => ({ results: sqlite.prepare(sql).all(...args) }),
    run: async () => sqlite.prepare(sql).run(...args),
  });
  return { prepare: (sql: string) => statement(sql), batch: vi.fn() };
}

const COLUMNS = [
  "id TEXT PRIMARY KEY",
  "username TEXT",
  "display_name TEXT",
  "avatar_url TEXT",
  "avatar_display TEXT",
  "updated_at TEXT",
  "bio TEXT",
  "pronouns TEXT",
  "status TEXT",
  "custom_status TEXT",
  "banner_url TEXT",
  "banner_content_type TEXT",
  "nameplate_url TEXT",
  "nameplate_content_type TEXT",
  "profile_accent_color TEXT",
  "profile_background_color TEXT",
  "profile_banner_color TEXT",
  "display_name_style TEXT",
  "theme_preference TEXT",
  "theme_sync_enabled INTEGER",
  "media_content_filter TEXT",
  "sound_settings TEXT",
  "created_at TEXT",
];

let sqlite: SqliteDatabase;

function insertUser(id: string, username: string) {
  sqlite
    .prepare(
      "INSERT INTO users (id, username, display_name, created_at) VALUES (?, ?, ?, '2026-01-01')",
    )
    .run(id, username, username);
}

function userRow(id: string) {
  return sqlite.prepare("SELECT * FROM users WHERE id = ?").get(id) ?? null;
}

describe("first sign-in", () => {
  beforeEach(() => {
    sqlite = new DatabaseSync(":memory:");
    sqlite.exec(`CREATE TABLE users (${COLUMNS.join(", ")})`);
    sqlite.exec(
      "CREATE UNIQUE INDEX idx_users_username_lower ON users(lower(username))",
    );
    mocks.getDB.mockReturnValue(d1(sqlite));
    mocks.requireAuth.mockResolvedValue({ userId: "attackerKovaId0001" });
    mocks.getMe.mockRejectedValue(new ServiceError("User not found", 404));
  });

  it("never merges an existing account into a new sign-in with a matching name", async () => {
    insertUser("victimKovaId00001", "victim");
    const victimBefore = userRow("victimKovaId00001");
    mocks.getCurrentUser.mockResolvedValue({
      id: "attackerKovaId0001",
      email: "victim@attacker.test",
      username: "victim",
      name: "victim",
    });

    const response = await GET({
      request: new Request("https://meet.test/api/users/me"),
    });

    expect(response.status).toBe(200);
    expect(await response.json()).toMatchObject({ id: "attackerKovaId0001" });
    expect(userRow("victimKovaId00001")).toEqual(victimBefore);
    expect(userRow("attackerKovaId0001")).toMatchObject({
      id: "attackerKovaId0001",
      display_name: "victim",
    });
    // The name stays with the account that held it.
    expect(
      (userRow("attackerKovaId0001") as { username: string }).username,
    ).not.toBe("victim");
  });
});
