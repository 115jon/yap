// The global gateway's /sessions/signed-out: when Kova sessions are signed
// out from another device, the chat sockets they opened close (4004, which
// clients retry into a ticket request that finds the session gone).
import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";

import {
  appendRealtimeAdmissionHeaders,
  createRealtimeAdmissionContext,
} from "../realtime-admission";
import { SESSIONS_SIGNED_OUT_PATH } from "../meeting-room";

const USER = "user-signed-out";

function gateway(name: string) {
  return env.MEETING_ROOM.get(env.MEETING_ROOM.idFromName(name));
}

/** One accepted chat socket for `subject`, from a ticket naming `authSession`. */
async function chatTicket(subject: string, authSession?: string) {
  return createRealtimeAdmissionContext({
    accessMode: "authenticated",
    audience: "global",
    expiresAt: Date.now() + 60_000,
    nonce: crypto.randomUUID(),
    roomSlug: "global-gateway",
    subject,
    ...(authSession ? { authSession } : {}),
  });
}

function upgrade(
  name: string,
  context: Awaited<ReturnType<typeof chatTicket>>,
) {
  return gateway(name).fetch("https://internal/api/gateway?v=1", {
    headers: appendRealtimeAdmissionHeaders(
      new Headers({ Upgrade: "websocket" }),
      context,
    ),
  });
}

async function chatSocket(
  name: string,
  subject: string,
  authSession?: string,
): Promise<{ socket: WebSocket; closed: Promise<number> }> {
  const response = await upgrade(name, await chatTicket(subject, authSession));
  const socket = response.webSocket;
  if (!socket) throw new Error(`no socket (${response.status})`);
  socket.accept();
  const closed = new Promise<number>((resolve) =>
    socket.addEventListener("close", (event) => resolve(event.code)),
  );
  return { socket, closed };
}

function signedOut(name: string, body: unknown) {
  return gateway(name).fetch(`https://internal${SESSIONS_SIGNED_OUT_PATH}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

/** The code a socket closed with, or null if it is still open after a beat. */
async function closeCode(closed: Promise<number>): Promise<number | null> {
  return Promise.race([
    closed,
    new Promise<null>((resolve) => setTimeout(() => resolve(null), 200)),
  ]);
}

describe("/sessions/signed-out", () => {
  it("closes the one signed-out session's chat sockets", async () => {
    const name = `gateway-${crypto.randomUUID()}`;
    const phone = await chatSocket(name, USER, "sess-phone");
    const laptop = await chatSocket(name, USER, "sess-laptop");
    const legacy = await chatSocket(name, USER);
    const someoneElse = await chatSocket(name, "user-other", "sess-phone");

    const response = await signedOut(name, {
      user_id: USER,
      auth_session_id: "sess-phone",
    });

    expect(await response.json()).toEqual({ closed: 1 });
    expect(await closeCode(phone.closed)).toBe(4004);
    expect(await closeCode(laptop.closed)).toBeNull();
    expect(await closeCode(legacy.closed)).toBeNull();
    expect(await closeCode(someoneElse.closed)).toBeNull();
    for (const s of [laptop, legacy, someoneElse]) s.socket.close(1000);
  });

  it("closes every other session's sockets, ticketless ones too, and keeps the caller's", async () => {
    const name = `gateway-${crypto.randomUUID()}`;
    const caller = await chatSocket(name, USER, "sess-here");
    const phone = await chatSocket(name, USER, "sess-phone");
    const legacy = await chatSocket(name, USER);

    const response = await signedOut(name, {
      user_id: USER,
      except_auth_session_id: "sess-here",
    });

    expect(await response.json()).toEqual({ closed: 2 });
    expect(await closeCode(phone.closed)).toBe(4004);
    expect(await closeCode(legacy.closed)).toBe(4004);
    expect(await closeCode(caller.closed)).toBeNull();
    caller.socket.close(1000);
  });

  it("refuses a ticket issued before its session was signed out, not a later one", async () => {
    const name = `gateway-${crypto.randomUUID()}`;
    const early = await chatTicket(USER, "sess-phone");
    const keeper = await chatTicket(USER, "sess-here");

    await signedOut(name, { user_id: USER, auth_session_id: "sess-phone" });

    const refused = await upgrade(name, early);
    expect(refused.status).toBe(401);
    expect(refused.webSocket).toBeNull();
    const kept = await upgrade(name, keeper);
    expect(kept.webSocket).not.toBeNull();
    kept.webSocket?.accept();
    kept.webSocket?.close(1000);
  });

  it("refuses a body without a user or with both filters", async () => {
    const name = `gateway-${crypto.randomUUID()}`;
    for (const body of [
      { auth_session_id: "x" },
      { user_id: USER },
      { user_id: USER, auth_session_id: "x", except_auth_session_id: "y" },
    ]) {
      expect((await signedOut(name, body)).status).toBe(400);
    }
  });
});
