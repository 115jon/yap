import { afterEach, describe, expect, it, vi } from "vitest";

import { createMockD1 } from "@/lib/__tests__/mock-d1";
import type { FcmSendResult } from "@/server/fcm";

import {
  buildNativeMessagePush,
  deletePushToken,
  deleteSessionPushTokens,
  hasNotificationsSinceLastRead,
  previewOf,
  sendMessagePushes,
  sendReadPushes,
  upsertPushToken,
  type MessagePush,
} from "../push.service";

const MENTION: MessagePush = {
  recipientId: "user-2",
  notificationId: "n-1",
  type: "mention",
  channelId: "chan-1",
  serverId: "srv-1",
  messageId: "msg-1",
  senderId: "user-9",
  sentAt: "2026-11-01T10:00:00.000Z",
};

/** What the message, its sender, channel and server are called. */
const TEXT = ["hey", "look at this", "Ada", "general", "Ralph", "avatar"];

/** Every key and string value in an FCM message, nested blocks included. */
function stringsIn(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (value && typeof value === "object") {
    return Object.entries(value).flatMap(([key, v]) => [key, ...stringsIn(v)]);
  }
  return [];
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("previewOf", () => {
  it("folds whitespace", () => {
    expect(previewOf("mention", "hey   @bob\n\nlook at this")).toBe(
      "hey @bob look at this",
    );
  });

  it("trims a long preview with an ellipsis, at the old 180", () => {
    const body = previewOf("mention", "a".repeat(500));

    expect(body).toHaveLength(180);
    expect(body.endsWith("…")).toBe(true);
  });

  it("does not cut a character in two", () => {
    const body = previewOf("dm", String.fromCodePoint(0x1f600).repeat(300));

    expect(Array.from(body)).toHaveLength(180);
    // A lone surrogate would show as a replacement character.
    expect(body).not.toMatch(/[\uD800-\uDBFF](?![\uDC00-\uDFFF])/);
  });

  it("names an empty message by its kind", () => {
    expect(previewOf("dm", "  ")).toBe("Sent an attachment");
    expect(previewOf("mention", "")).toBe("Mentioned you");
    expect(previewOf("reply", "")).toBe("Replied to you");
  });
});

describe("no message text reaches FCM", () => {
  it.each(["mention", "reply", "dm"] as const)(
    "a %s push holds ids only",
    (type) => {
      const strings = stringsIn(
        buildNativeMessagePush({ ...MENTION, type }),
      ).join("\n");
      for (const text of TEXT) expect(strings).not.toContain(text);
    },
  );

  it("sends no text to any device", async () => {
    const db = createMockD1();
    db.mockQuery("FROM push_tokens t", {
      results: [
        { user_id: "user-2", token: "phone" },
        { user_id: "user-2", token: "tablet" },
        { user_id: "user-2", token: "laptop" },
      ],
    });
    const send = vi.fn(
      async (_token: string, _message: unknown): Promise<FcmSendResult> => ({
        ok: true,
      }),
    );

    await sendMessagePushes(db as never, { send }, [MENTION]);

    expect(send).toHaveBeenCalledTimes(3);
    for (const [, message] of send.mock.calls) {
      const strings = stringsIn(message).join("\n");
      for (const text of TEXT) expect(strings).not.toContain(text);
      expect(strings).not.toMatch(/conversation|serverName|senderAvatar/);
      // Only the generic line an older build draws: no name or text.
      const data = (message as { data?: Record<string, string> }).data ?? {};
      if ("senderName" in data) expect(data.senderName).toBe("Yap");
      if ("preview" in data) expect(data.preview).toBe("New mention");
    }
  });
});

describe("push tokens", () => {
  it("upserts by token, re-owning it to the caller", async () => {
    const db = createMockD1();

    await upsertPushToken(db as never, "user-1", "id-1", {
      token: "tok",
      platform: "android",
      appVariant: "development",
      sessionId: "session-1",
    });

    db.assertCalledWith(
      /ON CONFLICT\(token\) DO UPDATE SET\s+user_id = excluded\.user_id/,
      [
        "id-1",
        "user-1",
        "tok",
        "android",
        "development",
        "session-1",
        expect.any(String),
        expect.any(String),
      ],
    );
  });

  it("deletes only the caller's own token", async () => {
    const db = createMockD1();

    await deletePushToken(db as never, "user-1", "tok");

    db.assertCalledWith(
      "DELETE FROM push_tokens WHERE token = ? AND user_id = ?",
      ["tok", "user-1"],
    );
  });

  it("deletes a signed-out session's tokens and the unbound ones", async () => {
    const db = createMockD1();

    await deleteSessionPushTokens(db as never, "user-1", {
      sessionId: "phone",
    });
    await deleteSessionPushTokens(db as never, "user-1", {
      allExcept: "laptop",
    });

    db.assertCalledWith(
      /WHERE user_id = \? AND \(session_id = \? OR session_id IS NULL\)/,
      ["user-1", "phone"],
    );
    db.assertCalledWith(
      /WHERE user_id = \? AND \(session_id != \? OR session_id IS NULL\)/,
      ["user-1", "laptop"],
    );
  });
});

describe("sendMessagePushes", () => {
  function senderAnswering(answer: (token: string) => FcmSendResult) {
    return { send: vi.fn(async (token: string) => answer(token)) };
  }

  it("sends to every device of the recipient and deletes dead tokens", async () => {
    vi.spyOn(console, "warn").mockImplementation(() => {});
    const db = createMockD1();
    db.mockQuery("FROM push_tokens t", {
      results: [
        { user_id: "user-2", token: "live" },
        { user_id: "user-2", token: "dead" },
        { user_id: "user-2", token: "flaky" },
      ],
    });
    const sender = senderAnswering((token) => {
      if (token === "dead") return { ok: false, status: 404, staleToken: true };
      if (token === "flaky") {
        return { ok: false, status: 503, staleToken: false };
      }
      return { ok: true };
    });

    await sendMessagePushes(db as never, sender, [MENTION]);

    expect(sender.send.mock.calls.map(([token]) => token).sort()).toEqual([
      "dead",
      "flaky",
      "live",
    ]);
    db.assertCalledWith("FROM push_tokens t", ["user-2"]);
    db.assertCalledWith("DELETE FROM push_tokens WHERE token = ?", ["dead"]);
    expect(db.getCalls("DELETE FROM push_tokens")).toHaveLength(1);
  });

  it("does nothing without a sender and never throws", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const db = createMockD1();

    await sendMessagePushes(db as never, null, [MENTION]);
    expect(db.calls).toHaveLength(0);

    const broken = {
      prepare: () => {
        throw new Error("D1 down");
      },
    };
    await expect(
      sendMessagePushes(
        broken as never,
        senderAnswering(() => ({ ok: true })),
        [MENTION],
      ),
    ).resolves.toBeUndefined();
  });
});

describe("buildNativeMessagePush", () => {
  it("is data-only with ids for a channel mention", () => {
    const message = buildNativeMessagePush(MENTION);

    expect(message).not.toHaveProperty("notification");
    expect(message.android).toEqual({ priority: "HIGH", ttl: "86400s" });
    expect(message.data).toEqual({
      ralph: "2",
      kind: "message",
      type: "mention",
      channelId: "chan-1",
      messageId: "msg-1",
      notificationId: "n-1",
      serverId: "srv-1",
      tag: "channel:chan-1",
      senderId: "user-9",
      sentAt: "2026-11-01T10:00:00.000Z",
      senderName: "Yap",
      preview: "New mention",
    });
  });

  it("leaves the server out for a DM", () => {
    const { data } = buildNativeMessagePush({
      ...MENTION,
      type: "dm",
      serverId: null,
    });

    expect(data.type).toBe("dm");
    expect(data).not.toHaveProperty("serverId");
  });
});

describe("sendMessagePushes formats", () => {
  it("sends every token the data-only push", async () => {
    const db = createMockD1();
    db.mockQuery("FROM push_tokens t", {
      results: [
        { user_id: "user-2", token: "phone" },
        { user_id: "user-2", token: "tablet" },
      ],
    });
    const send = vi.fn(async (): Promise<FcmSendResult> => ({ ok: true }));

    await sendMessagePushes(db as never, { send }, [MENTION]);

    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenCalledWith("phone", buildNativeMessagePush(MENTION));
    expect(send).toHaveBeenCalledWith(
      "tablet",
      buildNativeMessagePush(MENTION),
    );
  });

  it("asks only for recipients who are not on Do Not Disturb", async () => {
    const db = createMockD1();
    const send = vi.fn(async (): Promise<FcmSendResult> => ({ ok: true }));

    await sendMessagePushes(db as never, { send }, [MENTION]);

    // A DND user's tokens are filtered out in SQL, so none come back.
    expect(send).not.toHaveBeenCalled();
    const [call] = db.getCalls("FROM push_tokens t");
    expect(call.sql).toMatch(/COALESCE\(u\.status, 'online'\) != 'dnd'/);
  });
});

describe("sendReadPushes", () => {
  const ok = async (): Promise<FcmSendResult> => ({ ok: true });

  it("sends a data-only read push to every token", async () => {
    const db = createMockD1();
    db.mockQuery("FROM push_tokens", { results: [{ token: "new" }] });
    const send = vi.fn(ok);

    await sendReadPushes(
      db as never,
      { send },
      "read-user-1",
      "chan-1",
      "2026-11-01T10:00:00.000Z",
      1_000,
    );

    db.assertCalledWith("SELECT token FROM push_tokens WHERE user_id = ?", [
      "read-user-1",
    ]);
    expect(send).toHaveBeenCalledWith("new", {
      data: {
        ralph: "2",
        kind: "read",
        channelId: "chan-1",
        readAt: "2026-11-01T10:00:00.000Z",
      },
      android: { priority: "NORMAL", ttl: "3600s" },
    });
  });

  it("sends once per user and channel per 10 seconds", async () => {
    const db = createMockD1();
    db.mockQuery("FROM push_tokens", { results: [{ token: "new" }] });
    const send = vi.fn(ok);
    const read = (channel: string, at: number) =>
      sendReadPushes(db as never, { send }, "read-user-2", channel, "t", at);

    await read("chan-1", 50_000);
    await read("chan-1", 59_999);
    await read("chan-2", 59_999);
    await read("chan-1", 60_000);

    expect(send).toHaveBeenCalledTimes(3);
  });

  it("deletes dead tokens and never throws", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const db = createMockD1();
    db.mockQuery("FROM push_tokens", { results: [{ token: "dead" }] });

    await sendReadPushes(
      db as never,
      { send: async () => ({ ok: false, status: 404, staleToken: true }) },
      "read-user-3",
      "chan-1",
      "t",
      1_000,
    );
    db.assertCalledWith("DELETE FROM push_tokens WHERE token = ?", ["dead"]);

    const broken = {
      prepare: () => {
        throw new Error("D1 down");
      },
    };
    await expect(
      sendReadPushes(
        broken as never,
        { send: ok },
        "read-user-4",
        "chan-1",
        "t",
        1_000,
      ),
    ).resolves.toBeUndefined();
    await expect(
      sendReadPushes(db as never, null, "u", "c", "t"),
    ).resolves.toBeUndefined();
  });
});

describe("hasNotificationsSinceLastRead", () => {
  it("compares with julianday, so both timestamp formats work", async () => {
    const db = createMockD1();
    db.mockQuery("FROM notifications", { found: 1 });

    await expect(
      hasNotificationsSinceLastRead(db as never, "user-1", "chan-1"),
    ).resolves.toBe(true);

    db.assertCalledWith(/julianday\(created_at\) > COALESCE/, [
      "user-1",
      "chan-1",
      "user-1",
      "chan-1",
      "user-1",
      "chan-1",
      expect.stringMatching(/^\d{4}-\d{2}-\d{2}$/),
      "user-1",
      "chan-1",
    ]);
  });

  it("is false when nothing is newer than the last read", async () => {
    const db = createMockD1();
    db.mockQuery("FROM notifications", null);

    await expect(
      hasNotificationsSinceLastRead(db as never, "user-1", "chan-1"),
    ).resolves.toBe(false);
  });
});
