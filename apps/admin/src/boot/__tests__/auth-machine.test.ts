import { describe, expect, it } from "vitest";

import {
  authReducer,
  initialAuthState,
  isConsolePath,
  type AuthEvent,
  type AuthState,
} from "../auth-machine";

const session = { user_id: "admin-1" };
const run = (events: AuthEvent[], from: AuthState = initialAuthState) =>
  events.reduce(authReducer, from);

describe("the admin page's auth machine", () => {
  it("opens the console at once with a cookie from an earlier visit", () => {
    const state = run([{ type: "cookie-ok", session }]);
    expect(state).toMatchObject({ phase: "admin", session, epoch: 1 });
  });

  it("signs in: signed-out, verifying, admin", () => {
    const state = run([
      { type: "cookie-missing" },
      { type: "kova-signed-in" },
      { type: "verified", session },
    ]);
    expect(state).toMatchObject({ phase: "admin", session, epoch: 1 });
  });

  it("refuses a non-admin: denied, no console", () => {
    const state = run([
      { type: "cookie-missing" },
      { type: "kova-signed-in" },
      { type: "refused" },
    ]);
    expect(state).toMatchObject({ phase: "denied", session: null });
    // Signing in again from the neutral page verifies again.
    expect(authReducer(state, { type: "kova-signed-in" }).phase).toBe(
      "verifying",
    );
  });

  it("keeps the console's session while the session is renewed mid-use", () => {
    const lost = run([
      { type: "cookie-ok", session },
      { type: "session-lost" },
    ]);
    expect(lost).toMatchObject({ phase: "signed-out", session, lost: true });
    const back = run(
      [{ type: "kova-signed-in" }, { type: "verified", session }],
      lost,
    );
    expect(back).toMatchObject({ phase: "admin", lost: false, epoch: 2 });
  });

  it("takes one mid-use refusal as a lapsed Kova session, the next as a no", () => {
    const lost = run([
      { type: "cookie-ok", session },
      { type: "session-lost" },
      { type: "kova-signed-in" },
      { type: "refused" },
    ]);
    expect(lost).toMatchObject({ phase: "signed-out", session, retried: true });
    const no = run([{ type: "kova-signed-in" }, { type: "refused" }], lost);
    expect(no).toMatchObject({ phase: "denied", session: null });
  });

  it("does not loop when the server cannot be reached", () => {
    const state = run([
      { type: "cookie-missing" },
      { type: "kova-signed-in" },
      { type: "unreachable" },
    ]);
    expect(state).toMatchObject({ phase: "signed-out", unreachable: true });
  });

  it("drops the session on sign-out", () => {
    const state = run([{ type: "cookie-ok", session }, { type: "signed-out" }]);
    expect(state).toMatchObject({ phase: "signed-out", session: null });
  });

  it("ignores events that do not fit the phase", () => {
    const admin = run([{ type: "cookie-ok", session }]);
    expect(authReducer(admin, { type: "verified", session })).toBe(admin);
    expect(authReducer(admin, { type: "cookie-missing" })).toBe(admin);
    const out = run([{ type: "cookie-missing" }]);
    expect(authReducer(out, { type: "session-lost" })).toBe(out);
  });

  it("returns only to same-origin app paths", () => {
    for (const path of ["/people", "/people/u1", "/live", "/audit/"]) {
      expect(isConsolePath(path), path).toBe(true);
    }
    for (const path of [
      "/",
      "/people/u1/x",
      "//evil.test",
      "/\\evil.test",
      "https://evil.test/live",
      "/live?x=1",
    ]) {
      expect(isConsolePath(path), path).toBe(false);
    }
  });
});
