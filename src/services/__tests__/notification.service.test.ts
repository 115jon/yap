import { beforeEach, describe, expect, it } from "vitest";
import { createMockD1 } from "../../lib/__tests__/mock-d1";
import {
  clearNotifications,
  listNotifications,
  markNotificationsRead,
} from "../notification.service";

// ─── Fixtures ────────────────────────────────────────────────────────────────

const NOW = "2026-02-28T00:00:00.000Z";
const USER_ID = "user_abc";

function notifRow(overrides: Record<string, unknown> = {}) {
  return {
    id: "notif_1",
    type: "mention",
    channel_id: "chan_1",
    server_id: "server_1",
    message_id: "msg_1",
    from_user_id: "user_xyz",
    content: "Hello @you",
    is_read: 0,
    created_at: NOW,
    from_username: "alice",
    from_avatar_url: null,
    channel_name: "general",
    server_name: "My Server",
    ...overrides,
  };
}

// ─── listNotifications ───────────────────────────────────────────────────────

describe("listNotifications", () => {
  let db: ReturnType<typeof createMockD1>;
  const PAGE = /FROM notifications n\s+LEFT JOIN users/;

  // A member of server_1 who can see chan_1 (the @everyone role grants
  // VIEW_CHANNELS); later rules win, so a test narrows these.
  beforeEach(() => {
    db = createMockD1();
    db.mockQuery(/GROUP BY n\.channel_id/, {
      results: [{ channel_id: "chan_1", unread: 1 }],
    });
    db.mockQuery(
      /SELECT id, server_id, channel_type FROM channels WHERE id IN/,
      {
        results: [
          { id: "chan_1", server_id: "server_1", channel_type: "text" },
        ],
      },
    );
    db.mockQuery(/FROM server_members sm/, {
      results: [
        {
          server_id: "server_1",
          id: "role_everyone",
          permissions: 256,
          is_default: 1,
        },
      ],
    });
    db.mockQuery(/FROM channel_permission_overrides/, { results: [] });
  });

  it("returns formatted notifications and unread count", async () => {
    db.mockQuery(PAGE, {
      results: [notifRow(), notifRow({ id: "notif_2", is_read: 1 })],
    });

    const result = await listNotifications(db as any, USER_ID, {
      limit: 50,
      unreadOnly: false,
    });

    expect(result.notifications).toHaveLength(2);
    expect(result.unread_count).toBe(1);
    const n = result.notifications[0];
    expect(n.id).toBe("notif_1");
    expect(n.from_user.username).toBe("alice");
    expect(n.is_read).toBe(false);
  });

  it("leaves out a block either way, in the list and the count", async () => {
    db.mockQuery(PAGE, { results: [] });

    await listNotifications(db as any, USER_ID, {
      limit: 50,
      unreadOnly: false,
    });

    const calls = db.getCalls(/FROM notifications n/);
    expect(calls).toHaveLength(2);
    for (const call of calls) {
      expect(call.sql).toContain(
        "SELECT target_user_id FROM relationships WHERE type = 1 AND user_id = ?",
      );
      expect(call.sql).toContain(
        "SELECT user_id FROM relationships WHERE type = 1 AND target_user_id = ?",
      );
      expect(call.bindings.slice(0, 3)).toEqual([USER_ID, USER_ID, USER_ID]);
    }
  });

  it("filters the page in SQL to the channels the user can see", async () => {
    db.mockQuery(PAGE, { results: [] });
    db.mockQuery(/GROUP BY n\.channel_id/, {
      results: [
        { channel_id: "chan_1", unread: 1 },
        { channel_id: "chan_private", unread: 3 },
        { channel_id: "chan_other_server", unread: 2 },
        { channel_id: "dm_1", unread: 0 },
        { channel_id: "dm_2", unread: 4 },
        { channel_id: null, unread: 5 },
      ],
    });
    db.mockQuery(
      /SELECT id, server_id, channel_type FROM channels WHERE id IN/,
      {
        results: [
          { id: "chan_1", server_id: "server_1", channel_type: "text" },
          { id: "chan_private", server_id: "server_1", channel_type: "text" },
          {
            id: "chan_other_server",
            server_id: "server_2",
            channel_type: "text",
          },
          { id: "dm_1", server_id: null, channel_type: "dm" },
          { id: "dm_2", server_id: null, channel_type: "dm" },
        ],
      },
    );
    db.mockQuery(/FROM dm_recipients/, { results: [{ channel_id: "dm_1" }] });
    // @everyone is denied VIEW_CHANNELS in chan_private; not a member of
    // server_2, so no roles there.
    db.mockQuery(/FROM channel_permission_overrides/, {
      results: [
        {
          channel_id: "chan_private",
          target_id: "role_everyone",
          target_type: "role",
          allow: 0,
          deny: 256,
        },
      ],
    });

    const result = await listNotifications(db as any, USER_ID, {
      limit: 20,
      unreadOnly: true,
    });

    // The LIMIT applies after the access filter, inside the query.
    const [page] = db.getCalls(PAGE);
    expect(page.sql).toContain("n.is_read = 0");
    expect(page.sql).toContain("json_each(?)");
    expect(page.bindings.slice(0, 3)).toEqual([USER_ID, USER_ID, USER_ID]);
    expect(JSON.parse(page.bindings[3] as string).sort()).toEqual([
      "chan_1",
      "dm_1",
    ]);
    expect(page.bindings[4]).toBe(20);
    // The unread list only reads channels with unread rows.
    const [aggregate] = db.getCalls(/GROUP BY n\.channel_id/);
    expect(aggregate.sql).toContain("n.is_read = 0");
    // chan_1 (1) + no channel (5).
    expect(result.unread_count).toBe(6);
  });

  it("checks access in one batch whatever the channel count", async () => {
    db.mockQuery(PAGE, { results: [] });
    db.mockQuery(/GROUP BY n\.channel_id/, {
      results: Array.from({ length: 250 }, (_, i) => ({
        channel_id: `chan_${i}`,
        unread: 1,
      })),
    });

    await listNotifications(db as any, USER_ID, {
      limit: 50,
      unreadOnly: false,
    });

    expect(
      db.getCalls(
        /SELECT id, server_id, channel_type FROM channels WHERE id IN/,
      ),
    ).toHaveLength(1);
    expect(db.getCalls(/FROM server_members sm/)).toHaveLength(1);
    expect(db.getCalls(/FROM channel_permission_overrides/)).toHaveLength(1);
    expect(db.getCalls(/FROM dm_recipients/)).toHaveLength(1);
  });

  it("surfaces a D1 error instead of an empty inbox", async () => {
    const failing = createMockD1();
    failing.batch = async () => {
      throw new Error("D1_ERROR: boom");
    };
    failing.mockQuery(/GROUP BY n\.channel_id/, {
      results: [{ channel_id: "chan_1", unread: 1 }],
    });

    await expect(
      listNotifications(failing as any, USER_ID, {
        limit: 50,
        unreadOnly: false,
      }),
    ).rejects.toThrow("D1_ERROR");
  });

  it("marks is_read as boolean correctly", async () => {
    db.mockQuery(PAGE, {
      results: [notifRow({ is_read: 1 })],
    });

    const result = await listNotifications(db as any, USER_ID, {
      limit: 50,
      unreadOnly: false,
    });

    expect(result.notifications[0].is_read).toBe(true);
  });

  it("caps limit at 100", async () => {
    db.mockQuery(PAGE, { results: [] });

    await listNotifications(db as any, USER_ID, {
      limit: 999,
      unreadOnly: false,
    });

    db.assertCalledWith(PAGE, [
      USER_ID,
      USER_ID,
      USER_ID,
      JSON.stringify(["chan_1"]),
      100,
    ]);
  });

  it("returns empty notifications and zero unread count", async () => {
    db.mockQuery(PAGE, { results: [] });
    db.mockQuery(/GROUP BY n\.channel_id/, { results: [] });

    const result = await listNotifications(db as any, USER_ID, {
      limit: 50,
      unreadOnly: false,
    });

    expect(result.notifications).toEqual([]);
    expect(result.unread_count).toBe(0);
  });

  it("falls back to username 'Unknown' when from_username is null", async () => {
    db.mockQuery(PAGE, {
      results: [notifRow({ from_username: null })],
    });

    const result = await listNotifications(db as any, USER_ID, {
      limit: 50,
      unreadOnly: false,
    });

    expect(result.notifications[0].from_user.username).toBe("Unknown");
  });
});

// ─── markNotificationsRead ───────────────────────────────────────────────────

describe("markNotificationsRead", () => {
  let db: ReturnType<typeof createMockD1>;

  beforeEach(() => {
    db = createMockD1();
  });

  it("marks all notifications read when all=true", async () => {
    await markNotificationsRead(db as any, USER_ID, { all: true });

    db.assertCalledWith("UPDATE notifications SET is_read = 1", [USER_ID]);
  });

  it("marks specific notifications read by ids", async () => {
    await markNotificationsRead(db as any, USER_ID, {
      ids: ["notif_1", "notif_2"],
    });

    db.assertCalledWith("UPDATE notifications SET is_read = 1", [
      USER_ID,
      "notif_1",
      "notif_2",
    ]);
  });

  it("throws 400 when neither ids nor all is provided", async () => {
    await expect(
      markNotificationsRead(db as any, USER_ID, {}),
    ).rejects.toHaveProperty("status", 400);
  });

  it("throws 400 when ids is an empty array", async () => {
    await expect(
      markNotificationsRead(db as any, USER_ID, { ids: [] }),
    ).rejects.toHaveProperty("status", 400);
  });
});

// ─── clearNotifications ──────────────────────────────────────────────────────

describe("clearNotifications", () => {
  let db: ReturnType<typeof createMockD1>;

  beforeEach(() => {
    db = createMockD1();
  });

  it("deletes all notifications for the user", async () => {
    const clearedAt = await clearNotifications(db as any, USER_ID);

    db.assertCalledWith("DELETE FROM notifications", [USER_ID]);
    db.assertCalledWith("INSERT INTO notification_clear_watermarks", [
      USER_ID,
      clearedAt,
    ]);
  });
});
