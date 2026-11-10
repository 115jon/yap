import { describe, expect, it } from "vitest";

import {
  initListenActivitySchema,
  ListenActivityTracker,
  topTrackOf,
  type ListenActivityRecord,
  type ListenActivitySql,
} from "./listen-activity";

interface Row {
  user_id: string;
  started_at: number;
  tracks: number;
  counts_json: string;
}

/** Just enough SQL for the statements the tracker runs, over a Map. */
function createFakeSql(): { sql: ListenActivitySql; rows: Map<string, Row> } {
  const rows = new Map<string, Row>();
  const sql: ListenActivitySql = {
    exec<T extends Record<string, unknown>>(
      query: string,
      ...params: unknown[]
    ): Iterable<T> {
      const text = query.replace(/\s+/g, " ").trim();
      if (text.startsWith("CREATE TABLE")) return [];
      if (text.startsWith("INSERT INTO listen_activity")) {
        const [userId, startedAt, countsJson] = params as [
          string,
          number,
          string,
        ];
        const existing = rows.get(userId);
        rows.set(
          userId,
          existing
            ? {
                ...existing,
                tracks: existing.tracks + 1,
                counts_json: countsJson,
              }
            : {
                user_id: userId,
                started_at: startedAt,
                tracks: 1,
                counts_json: countsJson,
              },
        );
        return [];
      }
      if (
        text.startsWith(
          "SELECT started_at, tracks, counts_json FROM listen_activity WHERE user_id = ?",
        )
      ) {
        const row = rows.get(params[0] as string);
        return row ? ([{ ...row }] as unknown as T[]) : [];
      }
      if (text.startsWith("SELECT user_id, started_at")) {
        return [...rows.values()].map((row) => ({ ...row })) as unknown as T[];
      }
      if (text.startsWith("DELETE FROM listen_activity WHERE user_id = ?")) {
        rows.delete(params[0] as string);
        return [];
      }
      if (text === "DELETE FROM listen_activity") {
        rows.clear();
        return [];
      }
      throw new Error(`Unexpected SQL: ${text}`);
    },
  };
  return { sql, rows };
}

function setup(channelId: string | null = "chan-1") {
  const { sql, rows } = createFakeSql();
  initListenActivitySchema(sql);
  const recorded: ListenActivityRecord[] = [];
  const tracker = new ListenActivityTracker({
    sql,
    getRoomKey: () => "voice-srv-chan-1",
    getChannelId: () => channelId,
    record: (input) => recorded.push(input),
  });
  return { tracker, recorded, rows };
}

describe("ListenActivityTracker", () => {
  it("adds one track per start to every user present", () => {
    const { tracker, rows } = setup();
    tracker.trackStarted("Song A", ["u1", "u2"], 1_000);
    tracker.trackStarted("Song B", ["u1"], 2_000);

    expect(rows.get("u1")).toMatchObject({ started_at: 1_000, tracks: 2 });
    expect(JSON.parse(rows.get("u1")!.counts_json)).toEqual({
      "Song A": 1,
      "Song B": 1,
    });
    expect(rows.get("u2")).toMatchObject({ started_at: 1_000, tracks: 1 });
  });

  it("counts a user once however many participants they have", () => {
    const { tracker, rows } = setup();
    tracker.trackStarted("Song A", ["u1", "u1", "u1"], 1_000);
    expect(rows.get("u1")?.tracks).toBe(1);
  });

  it("flushes on leave with the top track, and only that user", () => {
    const { tracker, recorded, rows } = setup();
    tracker.trackStarted("Song A", ["u1", "u2"], 1_000);
    tracker.trackStarted("Song B", ["u1", "u2"], 2_000);
    tracker.trackStarted("Song A", ["u1"], 3_000);

    tracker.flushUser("u1", 9_000);

    expect(recorded).toEqual([
      {
        userId: "u1",
        sessionKey: "voice-srv-chan-1:1000",
        channelId: "chan-1",
        startedAt: 1_000,
        endedAt: 9_000,
        tracks: 3,
        topTrack: "Song A",
      },
    ]);
    expect(rows.has("u1")).toBe(false);
    expect(rows.has("u2")).toBe(true);
  });

  it("still flushes a short session (the writer skips under 3 tracks)", () => {
    const { tracker, recorded } = setup(null);
    tracker.trackStarted("Song A", ["u1"], 1_000);
    tracker.flushUser("u1", 2_000);

    expect(recorded).toHaveLength(1);
    expect(recorded[0]).toMatchObject({
      tracks: 1,
      channelId: null,
      topTrack: "Song A",
    });
  });

  it("flushes everyone when the queue clears", () => {
    const { tracker, recorded, rows } = setup();
    tracker.trackStarted("Song A", ["u1", "u2"], 1_000);
    tracker.flushAll(5_000);

    expect(recorded.map((record) => record.userId).sort()).toEqual([
      "u1",
      "u2",
    ]);
    expect(recorded.every((record) => record.endedAt === 5_000)).toBe(true);
    expect(rows.size).toBe(0);

    tracker.flushAll(6_000);
    expect(recorded).toHaveLength(2);
  });

  it("is idempotent: a second leave records nothing", () => {
    const { tracker, recorded } = setup();
    tracker.trackStarted("Song A", ["u1"], 1_000);
    tracker.flushUser("u1", 2_000);
    tracker.flushUser("u1", 3_000);
    expect(recorded).toHaveLength(1);
  });

  it("starts a fresh session after a flush", () => {
    const { tracker, recorded } = setup();
    tracker.trackStarted("Song A", ["u1"], 1_000);
    tracker.flushUser("u1", 2_000);
    tracker.trackStarted("Song B", ["u1"], 10_000);
    tracker.flushUser("u1", 11_000);

    expect(recorded.map((record) => record.sessionKey)).toEqual([
      "voice-srv-chan-1:1000",
      "voice-srv-chan-1:10000",
    ]);
  });

  it("counts a track with no title toward the total only", () => {
    const { tracker, rows } = setup();
    tracker.trackStarted("  ", ["u1"], 1_000);
    expect(rows.get("u1")).toMatchObject({ tracks: 1, counts_json: "{}" });
  });
});

describe("topTrackOf", () => {
  it("picks the most counted, the earliest on a tie, null when empty", () => {
    expect(topTrackOf({ a: 1, b: 3, c: 3 })).toBe("b");
    expect(topTrackOf({})).toBeNull();
  });
});
