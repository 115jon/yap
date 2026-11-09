import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";

import {
  AdminApiError,
  createHttpTransport,
  invalidateAfterWrite,
  isTransientError,
  SESSION_LOST,
} from "../admin-data";

const bare404 = () => new Response(null, { status: 404 });
const ok = (body: unknown) => Response.json(body);

/** A fake fetch answering from a queue per path; records every call. */
function fakeFetch(answers: Record<string, Array<() => Response>>) {
  const calls: string[] = [];
  const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = String(input);
    calls.push(`${init?.method ?? "GET"} ${path}`);
    const queue = answers[path];
    const next = queue && queue.length > 1 ? queue.shift()! : queue?.[0];
    if (!next) throw new TypeError("fetch failed");
    return next();
  }) as unknown as typeof globalThis.fetch;
  return { fetch, calls };
}

function transport(
  answers: Record<string, Array<() => Response>>,
  extra: Partial<Parameters<typeof createHttpTransport>[0]> = {},
) {
  const { fetch, calls } = fakeFetch(answers);
  const onSessionLost = vi.fn();
  const sleep = vi.fn(async () => {});
  const t = createHttpTransport({
    fetch,
    onSessionLost,
    sleep,
    ...extra,
  });
  return { t, calls, onSessionLost, sleep };
}

describe("the console's transport", () => {
  it("re-checks the session after one bare 404 and retries the request once", async () => {
    const { t, calls, onSessionLost } = transport({
      "/api/live": [bare404, () => ok({ live: true })],
      "/api/session": [() => ok({ user_id: "a" })],
    });
    await expect(t.get("/api/live")).resolves.toEqual({ live: true });
    expect(calls).toEqual([
      "GET /api/live",
      "GET /api/session",
      "GET /api/live",
    ]);
    expect(onSessionLost).not.toHaveBeenCalled();
  });

  it("reports the session lost only when /api/session bare-404s too", async () => {
    const { t, onSessionLost } = transport({
      "/api/live": [bare404],
      "/api/session": [bare404],
    });
    const error = await t.get("/api/live").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(AdminApiError);
    expect((error as AdminApiError).code).toBe(SESSION_LOST);
    expect(onSessionLost).toHaveBeenCalledTimes(1);
  });

  it("gives a section its own error when the retry still bare-404s", async () => {
    const { t, onSessionLost } = transport({
      "/api/users/x": [bare404],
      "/api/session": [() => ok({ user_id: "a" })],
    });
    const error = (await t
      .get("/api/users/x")
      .catch((e: unknown) => e)) as AdminApiError;
    expect(error.status).toBe(404);
    expect(error.code).toBeUndefined();
    expect(onSessionLost).not.toHaveBeenCalled();
  });

  it("treats an unreadable session check as transient, not as signed out", async () => {
    const { t, onSessionLost } = transport({
      "/api/live": [bare404],
      "/api/session": [() => new Response("bad gateway", { status: 502 })],
    });
    const error = await t.get("/api/live").catch((e: unknown) => e);
    expect(isTransientError(error)).toBe(true);
    expect(onSessionLost).not.toHaveBeenCalled();
  });

  it("shares one session check between requests that 404 together", async () => {
    const { t, calls } = transport({
      "/api/live": [bare404, () => ok(1)],
      "/api/users": [bare404, () => ok(2)],
      "/api/session": [() => ok({ user_id: "a" })],
    });
    await Promise.all([t.get("/api/live"), t.get("/api/users")]);
    expect(calls.filter((c) => c === "GET /api/session")).toHaveLength(1);
  });

  it("passes a JSON 404 (the API's own answer) straight through", async () => {
    const { t, calls } = transport({
      "/api/users/x": [
        () =>
          Response.json(
            { error: "No such account", code: "NOT_FOUND" },
            { status: 404 },
          ),
      ],
    });
    const error = (await t
      .get("/api/users/x")
      .catch((e: unknown) => e)) as AdminApiError;
    expect(error.message).toBe("No such account");
    expect(error.code).toBe("NOT_FOUND");
    expect(calls).toEqual(["GET /api/users/x"]);
  });

  it("retries a write on a transient failure with backoff, then succeeds", async () => {
    const onWrite = vi.fn();
    const { t, calls, sleep } = transport(
      {
        "/api/settings": [
          () => new Response(null, { status: 503 }),
          () => Response.json({ error: "slow down" }, { status: 429 }),
          () => ok({ saved: true }),
        ],
      },
      { onWrite },
    );
    await expect(t.send("PUT", "/api/settings", { a: 1 })).resolves.toEqual({
      saved: true,
    });
    expect(calls).toHaveLength(3);
    expect(sleep).toHaveBeenCalledTimes(2);
    expect(onWrite).toHaveBeenCalledWith("/api/settings");
  });

  it("does not repeat a write the API answered with a code, even a 502", async () => {
    // A catalog refresh that failed upstream was already run and logged.
    const { t, calls } = transport({
      "/api/catalog/refresh": [
        () =>
          Response.json(
            { error: "Upstream said 403", code: "CATALOG_REFRESH_FAILED" },
            { status: 502 },
          ),
      ],
    });
    await expect(t.send("POST", "/api/catalog/refresh")).rejects.toMatchObject({
      status: 502,
      code: "CATALOG_REFRESH_FAILED",
    });
    expect(calls).toHaveLength(1);
  });

  it("does not repeat a write whose answer was lost", async () => {
    const { t, calls } = transport({ "/api/users/u1/disconnect": [] });
    await expect(
      t.send("POST", "/api/users/u1/disconnect"),
    ).rejects.toMatchObject({ status: 0 });
    expect(calls).toHaveLength(1);
  });

  it("does not retry a write the server refused", async () => {
    const onWrite = vi.fn();
    const { t, calls } = transport(
      {
        "/api/settings": [
          () =>
            Response.json(
              { error: "Bad flag", code: "INVALID" },
              { status: 400 },
            ),
        ],
      },
      { onWrite },
    );
    await expect(t.send("PUT", "/api/settings", {})).rejects.toMatchObject({
      status: 400,
      code: "INVALID",
    });
    expect(calls).toHaveLength(1);
    expect(onWrite).not.toHaveBeenCalled();
  });

  it("hands a long Retry-After back to the caller instead of waiting", async () => {
    const { t, calls } = transport({
      "/api/catalog/refresh": [
        () =>
          Response.json(
            { error: "Wait a minute", code: "RATE_LIMITED" },
            { status: 429, headers: { "Retry-After": "60" } },
          ),
      ],
    });
    const error = (await t
      .send("POST", "/api/catalog/refresh")
      .catch((e: unknown) => e)) as AdminApiError;
    expect(error.status).toBe(429);
    expect(error.retryAfterS).toBe(60);
    expect(calls).toHaveLength(1);
  });
});

describe("invalidation after a write", () => {
  it("refetches what the write touched and the audit log", async () => {
    const client = new QueryClient();
    for (const path of [
      "/api/users",
      "/api/users/u1",
      "/api/users/u2",
      "/api/settings",
      "/api/audit?limit=100",
      "/api/live",
    ]) {
      client.setQueryData(["admin", path], { path });
    }
    const spy = vi.spyOn(client, "invalidateQueries");
    invalidateAfterWrite(client, "/api/users/u1/plan");
    const predicate = spy.mock.calls[0]![0]!.predicate!;
    const touched = client
      .getQueryCache()
      .getAll()
      .filter((query) => predicate(query))
      .map((query) => query.queryKey[1]);
    expect(touched.sort()).toEqual(
      ["/api/audit?limit=100", "/api/users", "/api/users/u1"].sort(),
    );
  });
});
