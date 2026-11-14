// /api/voice/socket-ticket on the Effect edge: the real route with a D1
// stand-in and test layers for Kova and the rate limiter. No module mocks.

import { beforeEach, describe, expect, it, vi } from "vitest";

import { createMockD1, type MockD1 } from "@/lib/__tests__/mock-d1";
import { PERMISSIONS } from "@/lib/permissions";
import type { WorkerBindings } from "@/server/bindings";
import { brokenAuth, memoryAuth, testServices } from "@/server/testing";
import { verifySocketTicket } from "@/lib/voice/socket-ticket";
import { socketTicketRoutes } from "@/server/routes/socket-ticket";

const ticketSecret = "test-realtime-ticket-secret";
const callsSecret = "test-calls-secret";

const TOKEN = "Bearer good-token";

let env: Partial<WorkerBindings>;
let db: MockD1;
let auth = memoryAuth({ [TOKEN]: "user-123" });

function routes() {
  return socketTicketRoutes(testServices({ auth, db, env }));
}

function socketTicketPost(args: { request: Request }) {
  return routes().POST(args);
}

/** channel-1: a voice channel in server-1 that user-123 may see and join. */
function voiceChannelInServer1() {
  db.mockQuery("SELECT id, server_id, channel_type FROM channels", {
    id: "channel-1",
    server_id: "server-1",
    channel_type: "voice",
  });
  db.mockQuery("SELECT 1 FROM server_members", { 1: 1 });
  db.mockQuery("JOIN member_roles", {
    results: [
      {
        id: "role-everyone",
        permissions: PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.CONNECT,
        position: 0,
        is_default: 1,
      },
    ],
  });
}

describe("socket ticket route", () => {
  beforeEach(() => {
    env = {
      CALLS_APP_SECRET: callsSecret,
      REALTIME_TICKET_SECRET: ticketSecret,
    };
    db = createMockD1();
    auth = memoryAuth({ [TOKEN]: "user-123" });
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });

  it("issues a no-store, audience-bound ticket for an authenticated global gateway", async () => {
    const response = await socketTicketPost({
      request: new Request("https://meet.test/api/voice/socket-ticket", {
        method: "POST",
        headers: { authorization: TOKEN },
        body: JSON.stringify({ audience: "global" }),
      }),
    });
    const body = (await response.json()) as {
      expires_at: number;
      media_bootstrap?: unknown;
      ticket: string;
    };

    expect(response.status).toBe(200);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(body.media_bootstrap).toBeUndefined();
    await expect(
      verifySocketTicket(body.ticket, ticketSecret, {
        accessMode: "authenticated",
        audience: "global",
        now: body.expires_at - 1,
        roomSlug: "global-gateway",
      }),
    ).resolves.toMatchObject({
      ok: true,
      claims: { subject: "user-123" },
    });
  });

  it("fails closed when deployment ticket configuration is incomplete", async () => {
    env = {};
    const response = await socketTicketPost({
      request: new Request("https://meet.test/api/voice/socket-ticket", {
        method: "POST",
        headers: { authorization: TOKEN },
        body: JSON.stringify({ audience: "global" }),
      }),
    });

    await expect(response.json()).resolves.toMatchObject({
      code: "REALTIME_TICKET_SECRET_MISSING",
    });
    expect(response.status).toBe(503);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
  });

  it("answers 401 with no-store when Kova finds no session", async () => {
    const response = await socketTicketPost({
      request: new Request("https://meet.test/api/voice/socket-ticket", {
        method: "POST",
        headers: { authorization: "Bearer expired-token" },
        body: JSON.stringify({ audience: "global" }),
      }),
    });

    expect(response.status).toBe(401);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    await expect(response.json()).resolves.toEqual({ error: "Unauthorized" });
  });

  it("answers 503 AUTH_UNAVAILABLE, not 401, when Kova cannot answer", async () => {
    auth = brokenAuth("HTTP 502");
    const response = await socketTicketPost({
      request: new Request("https://meet.test/api/voice/socket-ticket", {
        method: "POST",
        headers: { authorization: TOKEN },
        body: JSON.stringify({ audience: "global" }),
      }),
    });

    expect(response.status).toBe(503);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    expect(response.headers.get("Retry-After")).toBe("5");
    await expect(response.json()).resolves.toMatchObject({
      code: "AUTH_UNAVAILABLE",
    });
  });

  it("refuses the retired room and voice audiences", async () => {
    voiceChannelInServer1();
    for (const audience of ["room", "voice"]) {
      const response = await socketTicketPost({
        request: new Request("https://meet.test/api/voice/socket-ticket", {
          method: "POST",
          headers: { authorization: TOKEN },
          body: JSON.stringify({ audience, channelId: "channel-1" }),
        }),
      });

      expect(response.status).toBe(400);
      expect(response.headers.get("Cache-Control")).toBe("no-store");
      await expect(response.json()).resolves.toMatchObject({
        code: "INVALID_AUDIENCE",
      });
    }
  });
});
