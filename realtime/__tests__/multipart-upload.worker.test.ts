import { env } from "cloudflare:workers";
import { beforeAll, describe, expect, it, vi } from "vitest";

import migration from "../../migrations/040_pending_uploads.sql?raw";
import { PERMISSIONS } from "../../src/lib/permissions";
import { MAX_ATTACHMENT_BYTES, MB } from "../../src/lib/upload-limits";
import {
  attachmentLimitForUser,
  cleanupStalePendingUploads,
  createMultipartUploadHandlers,
  type MultipartUploadDeps,
} from "../../src/services/multipart-upload.service";

// The chunked-upload routes against miniflare's D1 and R2. Auth, channel
// access and permissions are stubs keyed by an `x-test-user` header; the
// SQL, the R2 multipart calls and every size check are the real ones.

const db = env.DB as unknown as D1Database;
const bucket = env.BUCKET as unknown as R2Bucket;
/** R2's smallest part, so a three-part file stays ~11 MB. */
const PART = 5 * MB;
const BASE = "https://yap.test";

beforeAll(async () => {
  for (const statement of migration.split(";")) {
    if (statement.trim()) await db.prepare(statement).run();
  }
  await db
    .prepare(
      `CREATE TABLE IF NOT EXISTS attachments (
        id TEXT PRIMARY KEY, message_id TEXT, channel_id TEXT,
        soundboard_server_id TEXT, sound_name TEXT, sound_emoji TEXT,
        sound_volume REAL DEFAULT 1.0, filename TEXT NOT NULL,
        file_key TEXT NOT NULL, content_type TEXT, size_bytes INTEGER NOT NULL,
        is_nsfw INTEGER NOT NULL DEFAULT 0, user_id TEXT NOT NULL,
        created_at TEXT NOT NULL)`,
    )
    .run();
  await db
    .prepare(
      `CREATE TABLE IF NOT EXISTS r2_cleanup_jobs (
        file_key TEXT PRIMARY KEY, attempts INTEGER NOT NULL DEFAULT 0,
        last_error TEXT, created_at TEXT, updated_at TEXT, next_attempt_at TEXT)`,
    )
    .run();
});

interface Fixture {
  user: string;
  other: string;
  channel: string;
  /** A channel `user` can't see. */
  hidden: string;
  /** A server channel where `user` lacks ATTACH_FILES. */
  readOnly: string;
}

function fixture(): Fixture {
  const tag = crypto.randomUUID();
  return {
    user: `u-${tag}`,
    other: `u-other-${tag}`,
    channel: `c-${tag}`,
    hidden: `c-hidden-${tag}`,
    readOnly: `c-ro-${tag}`,
  };
}

function handlers(f: Fixture, overrides: Partial<MultipartUploadDeps> = {}) {
  return createMultipartUploadHandlers({
    db,
    bucket,
    authenticate: async (request) => {
      const user = request.headers.get("x-test-user");
      return user
        ? { userId: user }
        : Response.json({ error: "Unauthorized" }, { status: 401 });
    },
    channelAccess: async (_userId, channelId) => {
      if (channelId === f.hidden) {
        return Response.json(
          { error: "Channel not found or access denied" },
          { status: 403 },
        );
      }
      return { serverId: channelId === f.readOnly ? "srv-ro" : "srv-1" };
    },
    serverPermissions: async (serverId) =>
      serverId === "srv-ro"
        ? PERMISSIONS.SEND_MESSAGES
        : PERMISSIONS.ATTACH_FILES,
    rateLimit: async () => null,
    attachmentLimit: () => 500 * MB,
    partSize: PART,
    ...overrides,
  });
}

function bytes(size: number): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(new ArrayBuffer(size));
  for (let offset = 0; offset < size; offset += 65_536) {
    crypto.getRandomValues(
      out.subarray(offset, Math.min(size, offset + 65_536)),
    );
  }
  return out;
}

async function sha256(
  data: ArrayBuffer | Uint8Array<ArrayBuffer>,
): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", data);
  return [...new Uint8Array(digest)]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function req(
  method: string,
  path: string,
  user: string | null,
  body?: BodyInit,
  headers: Record<string, string> = {},
): Request {
  return new Request(`${BASE}${path}`, {
    method,
    body,
    headers: { ...(user ? { "x-test-user": user } : {}), ...headers },
  });
}

async function startUpload(
  h: ReturnType<typeof handlers>,
  f: Fixture,
  size: number,
  extra: Record<string, unknown> = {},
) {
  const res = await h.create(
    req(
      "POST",
      `/api/channels/${f.channel}/uploads`,
      f.user,
      JSON.stringify({
        filename: "clip.mp4",
        size,
        content_type: "video/mp4",
        ...extra,
      }),
      { "content-type": "application/json" },
    ),
    { channelId: f.channel },
  );
  expect(res.status).toBe(201);
  return (await res.json()) as {
    upload_id: string;
    part_size: number;
    part_count: number;
    expires_at: string;
  };
}

function putPart(
  h: ReturnType<typeof handlers>,
  f: Fixture,
  uploadId: string,
  partNumber: number | string,
  data: Uint8Array<ArrayBuffer>,
  user: string | null = f.user,
) {
  // A real client's request carries Content-Length; one built in workerd
  // doesn't unless it is set.
  return h.part(
    req(
      "PUT",
      `/api/channels/${f.channel}/uploads/${uploadId}/parts/${partNumber}`,
      user,
      data,
      { "content-length": String(data.length) },
    ),
    { channelId: f.channel, uploadId, partNumber: String(partNumber) },
  );
}

function slicePart(
  data: Uint8Array<ArrayBuffer>,
  n: number,
): Uint8Array<ArrayBuffer> {
  return data.subarray((n - 1) * PART, Math.min(data.length, n * PART));
}

/**
 * The real bucket, with hooks around one multipart's calls: `afterPart`
 * runs once a part is stored, `complete` replaces the complete call.
 */
function hookedBucket(hooks: {
  afterPart?: () => Promise<void>;
  complete?: (
    real: R2MultipartUpload,
    parts: R2UploadedPart[],
  ) => Promise<R2Object>;
}): R2Bucket {
  return new Proxy(bucket, {
    get(target, prop) {
      if (prop === "resumeMultipartUpload") {
        return (key: string, uploadId: string) => {
          const real = target.resumeMultipartUpload(key, uploadId);
          return {
            key,
            uploadId,
            async uploadPart(n: number, value: ReadableStream) {
              const uploaded = await real.uploadPart(n, value);
              await hooks.afterPart?.();
              return uploaded;
            },
            complete: (parts: R2UploadedPart[]) =>
              hooks.complete
                ? hooks.complete(real, parts)
                : real.complete(parts),
            abort: () => real.abort(),
          };
        };
      }
      const value = Reflect.get(target, prop);
      return typeof value === "function" ? value.bind(target) : value;
    },
  }) as R2Bucket;
}

async function uploadStatus(uploadId: string) {
  const row = await db
    .prepare("SELECT status FROM pending_uploads WHERE id = ?")
    .bind(uploadId)
    .first<{ status: string }>();
  return row?.status ?? null;
}

async function pendingRows(uploadId: string) {
  const upload = await db
    .prepare("SELECT COUNT(*) AS n FROM pending_uploads WHERE id = ?")
    .bind(uploadId)
    .first<{ n: number }>();
  const parts = await db
    .prepare(
      "SELECT COUNT(*) AS n FROM pending_upload_parts WHERE upload_id = ?",
    )
    .bind(uploadId)
    .first<{ n: number }>();
  return { upload: upload?.n ?? 0, parts: parts?.n ?? 0 };
}

describe("chunked uploads: happy path", () => {
  it("assembles out-of-order parts into one object with the same bytes", async () => {
    const f = fixture();
    const h = handlers(f);
    const data = bytes(2 * PART + 1234);
    const session = await startUpload(h, f, data.length);
    expect(session.part_size).toBe(PART);
    expect(session.part_count).toBe(3);

    // Parts land in any order, two at once.
    const results = await Promise.all([
      putPart(h, f, session.upload_id, 3, slicePart(data, 3)),
      putPart(h, f, session.upload_id, 1, slicePart(data, 1)),
    ]);
    expect(results.map((r) => r.status)).toEqual([200, 200]);

    const status = await h.status(
      req(
        "GET",
        `/api/channels/${f.channel}/uploads/${session.upload_id}`,
        f.user,
      ),
      { channelId: f.channel, uploadId: session.upload_id },
    );
    expect(status.status).toBe(200);
    const statusBody = (await status.json()) as {
      parts: { part_number: number; size: number }[];
    };
    expect(statusBody.parts).toEqual([
      { part_number: 1, size: PART },
      { part_number: 3, size: 1234 },
    ]);

    expect(
      (await putPart(h, f, session.upload_id, 2, slicePart(data, 2))).status,
    ).toBe(200);

    const done = await h.complete(
      req(
        "POST",
        `/api/channels/${f.channel}/uploads/${session.upload_id}/complete`,
        f.user,
      ),
      { channelId: f.channel, uploadId: session.upload_id },
    );
    expect(done.status).toBe(201);
    const attachment = (await done.json()) as {
      id: string;
      file_url: string;
      file_name: string;
      file_size: number;
      content_type: string;
    };
    expect(attachment.file_size).toBe(data.length);
    expect(attachment.file_name).toBe("clip.mp4");
    expect(attachment.content_type).toBe("video/mp4");
    expect(attachment.file_url).toBe(
      `/api/attachments/${f.channel}/${attachment.id}/clip.mp4`,
    );

    const key = attachment.file_url.slice("/api/".length);
    const object = await bucket.get(key);
    expect(object?.size).toBe(data.length);
    expect(object?.httpMetadata?.contentType).toBe("video/mp4");
    expect(await sha256(await object!.arrayBuffer())).toBe(await sha256(data));

    const row = await db
      .prepare("SELECT * FROM attachments WHERE id = ?")
      .bind(attachment.id)
      .first<Record<string, unknown>>();
    expect(row).toMatchObject({
      channel_id: f.channel,
      message_id: null,
      user_id: f.user,
      size_bytes: data.length,
      file_key: key,
    });
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 0,
      parts: 0,
    });
  });

  it("accepts a re-sent part (a retry) and keeps the last copy", async () => {
    const f = fixture();
    const h = handlers(f);
    const data = bytes(PART + 10);
    const session = await startUpload(h, f, data.length);
    const first = bytes(PART);
    expect((await putPart(h, f, session.upload_id, 1, first)).status).toBe(200);
    expect(
      (await putPart(h, f, session.upload_id, 1, slicePart(data, 1))).status,
    ).toBe(200);
    expect(
      (await putPart(h, f, session.upload_id, 2, slicePart(data, 2))).status,
    ).toBe(200);
    const done = await h.complete(req("POST", "/x", f.user), {
      channelId: f.channel,
      uploadId: session.upload_id,
    });
    expect(done.status).toBe(201);
    const { file_url } = (await done.json()) as { file_url: string };
    const object = await bucket.get(file_url.slice("/api/".length));
    expect(await sha256(await object!.arrayBuffer())).toBe(await sha256(data));
  });

  it("keeps the key under the attachment's own folder whatever the name", async () => {
    const f = fixture();
    const h = handlers(f);
    const session = await startUpload(h, f, PART + 1, {
      filename: "../../avatars/x\\evil\u0000.png",
      content_type: "image/png\r\nX-Evil: 1",
    });
    const row = await db
      .prepare(
        "SELECT file_key, filename, content_type FROM pending_uploads WHERE id = ?",
      )
      .bind(session.upload_id)
      .first<{ file_key: string; filename: string; content_type: string }>();
    expect(row!.filename).toBe(".._.._avatars_x_evil_.png");
    expect(row!.file_key).toMatch(
      new RegExp(
        `^attachments/${f.channel}/[0-9a-f-]{36}/\\.\\._\\.\\._avatars_x_evil_\\.png$`,
      ),
    );
    expect(row!.content_type).toBe("application/octet-stream");
  });
});

describe("chunked uploads: auth and ownership", () => {
  it("answers 401 on every route without a session", async () => {
    const f = fixture();
    const h = handlers(f);
    const session = await startUpload(h, f, PART + 1);
    const params = { channelId: f.channel, uploadId: session.upload_id };
    const responses = await Promise.all([
      h.create(req("POST", "/x", null, "{}"), { channelId: f.channel }),
      h.status(req("GET", "/x", null), params),
      putPart(h, f, session.upload_id, 1, bytes(PART), null),
      h.complete(req("POST", "/x", null), params),
      h.abort(req("DELETE", "/x", null), params),
    ]);
    expect(responses.map((r) => r.status)).toEqual([401, 401, 401, 401, 401]);
  });

  it("hides one user's upload from another (404, nothing written)", async () => {
    const f = fixture();
    const h = handlers(f);
    const session = await startUpload(h, f, PART + 1);
    const params = { channelId: f.channel, uploadId: session.upload_id };
    const responses = await Promise.all([
      h.status(req("GET", "/x", f.other), params),
      putPart(h, f, session.upload_id, 1, bytes(PART), f.other),
      h.complete(req("POST", "/x", f.other), params),
      h.abort(req("DELETE", "/x", f.other), params),
    ]);
    expect(responses.map((r) => r.status)).toEqual([404, 404, 404, 404]);
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 1,
      parts: 0,
    });
  });

  it("binds the upload to its channel", async () => {
    const f = fixture();
    const h = handlers(f);
    const session = await startUpload(h, f, PART + 1);
    const elsewhere = `${f.channel}-other`;
    const res = await h.part(req("PUT", "/x", f.user, bytes(PART)), {
      channelId: elsewhere,
      uploadId: session.upload_id,
      partNumber: "1",
    });
    expect(res.status).toBe(404);
  });

  it("checks channel access and ATTACH_FILES", async () => {
    const f = fixture();
    const h = handlers(f);
    const body = JSON.stringify({ filename: "a.bin", size: PART + 1 });
    const hidden = await h.create(req("POST", "/x", f.user, body), {
      channelId: f.hidden,
    });
    expect(hidden.status).toBe(403);
    const readOnly = await h.create(req("POST", "/x", f.user, body), {
      channelId: f.readOnly,
    });
    expect(readOnly.status).toBe(403);
    expect(await readOnly.json()).toMatchObject({
      error: "You do not have permission to upload files",
    });
  });

  it("passes the rate limiter's answer through", async () => {
    const f = fixture();
    const rateLimit = vi.fn(async (_user: string, action: string) =>
      action === "part"
        ? Response.json({ error: "Too many requests" }, { status: 429 })
        : null,
    );
    const h = handlers(f, { rateLimit });
    const session = await startUpload(h, f, PART + 1);
    const res = await putPart(h, f, session.upload_id, 1, bytes(PART));
    expect(res.status).toBe(429);
    expect(rateLimit).toHaveBeenCalledWith(f.user, "create");
    expect(rateLimit).toHaveBeenCalledWith(f.user, "part");
  });

  it("at the cap, makes room by aborting the least recently active upload", async () => {
    const f = fixture();
    let clock = Date.parse("2032-01-01T00:00:00Z");
    const h = handlers(f, { maxOpenUploads: 2, now: () => clock });
    const a = await startUpload(h, f, PART + 1);
    clock += 1000;
    const b = await startUpload(h, f, PART + 1);
    clock += 1000;
    // A part makes `a` the more recently active one.
    expect((await putPart(h, f, a.upload_id, 1, bytes(PART))).status).toBe(200);
    clock += 1000;

    const c = await h.create(
      req(
        "POST",
        "/x",
        f.user,
        JSON.stringify({ filename: "c", size: PART + 1 }),
      ),
      { channelId: f.channel },
    );
    expect(c.status).toBe(201);
    expect(await pendingRows(b.upload_id)).toEqual({ upload: 0, parts: 0 });
    expect(await pendingRows(a.upload_id)).toEqual({ upload: 1, parts: 1 });
    // The evicted upload's client hears "gone" and starts over.
    expect((await putPart(h, f, b.upload_id, 1, bytes(PART))).status).toBe(404);
  });

  it("refuses with Retry-After only when every slot is mid-complete", async () => {
    const f = fixture();
    const h = handlers(f, { maxOpenUploads: 1 });
    const a = await startUpload(h, f, PART + 1);
    await db
      .prepare("UPDATE pending_uploads SET status = 'completing' WHERE id = ?")
      .bind(a.upload_id)
      .run();
    const res = await h.create(
      req(
        "POST",
        "/x",
        f.user,
        JSON.stringify({ filename: "b", size: PART + 1 }),
      ),
      { channelId: f.channel },
    );
    expect(res.status).toBe(429);
    expect(Number(res.headers.get("Retry-After"))).toBeGreaterThan(0);
    expect(await res.json()).toMatchObject({ code: "TOO_MANY_UPLOADS" });
  });
});

describe("chunked uploads: validation", () => {
  it("refuses a declared size over the limit at create, naming the limit", async () => {
    const f = fixture();
    const h = handlers(f, { attachmentLimit: () => 25 * MB });
    const res = await h.create(
      req(
        "POST",
        "/x",
        f.user,
        JSON.stringify({ filename: "a.bin", size: 25 * MB + 1 }),
      ),
      { channelId: f.channel },
    );
    expect(res.status).toBe(413);
    expect(await res.json()).toMatchObject({
      error: "File too large (max 25MB)",
    });
  });

  it("never goes past MAX_ATTACHMENT_BYTES, whatever limit it is given", async () => {
    const f = fixture();
    const h = handlers(f, { attachmentLimit: () => 10 * 1024 * MB });
    const res = await h.create(
      req(
        "POST",
        "/x",
        f.user,
        JSON.stringify({ filename: "a.bin", size: MAX_ATTACHMENT_BYTES + 1 }),
      ),
      { channelId: f.channel },
    );
    expect(res.status).toBe(413);
  });

  it.each([
    [{ filename: "", size: 10 }],
    [{ filename: "a", size: 0 }],
    [{ filename: "a", size: 1.5 }],
    [{ filename: "a", size: "10" }],
    [{ size: 10 }],
  ])("refuses a bad create body %j", async (body) => {
    const f = fixture();
    const res = await handlers(f).create(
      req("POST", "/x", f.user, JSON.stringify(body)),
      { channelId: f.channel },
    );
    expect(res.status).toBe(400);
  });

  it.each(["0", "4", "-1", "1.5", "abc", "01", "999999"])(
    "refuses part number %s",
    async (partNumber) => {
      const f = fixture();
      const h = handlers(f);
      const session = await startUpload(h, f, 2 * PART + 1);
      const res = await putPart(
        h,
        f,
        session.upload_id,
        partNumber,
        bytes(PART),
      );
      expect(res.status).toBe(400);
    },
  );

  it("refuses a part of the wrong length", async () => {
    const f = fixture();
    const h = handlers(f);
    const session = await startUpload(h, f, PART + 100);
    expect(
      (await putPart(h, f, session.upload_id, 1, bytes(PART - 1))).status,
    ).toBe(400);
    expect(
      (await putPart(h, f, session.upload_id, 1, bytes(PART + 1))).status,
    ).toBe(413);
    expect((await putPart(h, f, session.upload_id, 2, bytes(101))).status).toBe(
      413,
    );
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 1,
      parts: 0,
    });
  });

  it("checks the streamed length when there is no Content-Length", async () => {
    const f = fixture();
    const h = handlers(f);
    const session = await startUpload(h, f, PART + 100);
    const stream = (size: number) =>
      new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(bytes(size));
          controller.close();
        },
      });
    const params = {
      channelId: f.channel,
      uploadId: session.upload_id,
      partNumber: "2",
    };
    const tooShort = await h.part(req("PUT", "/x", f.user, stream(99)), params);
    expect(tooShort.status).toBe(400);
    const tooLong = await h.part(req("PUT", "/x", f.user, stream(101)), params);
    expect(tooLong.status).toBe(413);
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 1,
      parts: 0,
    });
    const exact = await h.part(req("PUT", "/x", f.user, stream(100)), params);
    expect(exact.status).toBe(200);
  });

  it("refuses a body that ends before its Content-Length", async () => {
    const f = fixture();
    const h = handlers(f);
    const session = await startUpload(h, f, PART + 100);
    const short = new Request(`${BASE}/x`, {
      method: "PUT",
      headers: { "x-test-user": f.user, "content-length": "100" },
      body: new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(bytes(40));
          controller.close();
        },
      }),
    });
    const res = await h.part(short, {
      channelId: f.channel,
      uploadId: session.upload_id,
      partNumber: "2",
    });
    expect(res.status).toBe(400);
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 1,
      parts: 0,
    });
  });
});

describe("chunked uploads: the plan decides the limit", () => {
  // The production limit: the uploader's row in user_entitlements, read by
  // the server on create and again on complete. Nothing in the request
  // speaks for the plan.
  beforeAll(async () => {
    await db
      .prepare(
        `CREATE TABLE IF NOT EXISTS user_entitlements (
          user_id TEXT PRIMARY KEY, tier TEXT NOT NULL, source TEXT NOT NULL,
          started_at TEXT NOT NULL, expires_at TEXT, stripe_customer_id TEXT,
          stripe_subscription_id TEXT, updated_at TEXT NOT NULL)`,
      )
      .run();
  });

  async function grant(userId: string, tier: "loud" | "founder") {
    await db
      .prepare(
        `INSERT INTO user_entitlements (user_id, tier, source, started_at, updated_at)
         VALUES (?, ?, 'grant', '2026-11-01T00:00:00.000Z', '2026-11-01T00:00:00.000Z')`,
      )
      .bind(userId, tier)
      .run();
  }

  function planHandlers(f: Fixture) {
    return handlers(f, {
      attachmentLimit: (userId) => attachmentLimitForUser(db, userId),
    });
  }

  function create(
    f: Fixture,
    size: number,
    extra: Record<string, unknown> = {},
  ) {
    return planHandlers(f).create(
      req(
        "POST",
        `/api/channels/${f.channel}/uploads`,
        f.user,
        JSON.stringify({ filename: "clip.mp4", size, ...extra }),
      ),
      { channelId: f.channel },
    );
  }

  it("lets a founder start a 200 MB upload", async () => {
    const f = fixture();
    await grant(f.user, "founder");
    const res = await create(f, 200 * MB);
    expect(res.status).toBe(201);
    expect(await res.json()).toMatchObject({ part_count: 40 });
  });

  it("refuses a free user's 30 MB upload, naming 25 MB", async () => {
    const f = fixture();
    const res = await create(f, 30 * MB);
    expect(res.status).toBe(413);
    expect(await res.json()).toMatchObject({
      error: "File too large (max 25MB)",
    });
  });

  it("ignores a plan the client claims for itself", async () => {
    const f = fixture();
    const res = await create(f, 30 * MB, {
      tier: "founder",
      entitlement: { tier: "founder" },
    });
    expect(res.status).toBe(413);
  });

  it("checks the stored plan again at complete", async () => {
    const f = fixture();
    await grant(f.user, "loud");
    const h = planHandlers(f);
    const data = bytes(25 * MB + PART);
    const session = await startUpload(h, f, data.length);
    for (let n = 1; n <= session.part_count; n++) {
      expect(
        (await putPart(h, f, session.upload_id, n, slicePart(data, n))).status,
      ).toBe(200);
    }
    // The plan lapsed mid-upload: the webhook removed the row.
    await db
      .prepare(`DELETE FROM user_entitlements WHERE user_id = ?`)
      .bind(f.user)
      .run();
    const res = await h.complete(req("POST", "/x", f.user), {
      channelId: f.channel,
      uploadId: session.upload_id,
    });
    expect(res.status).toBe(413);
    expect(await res.json()).toMatchObject({
      error: "File too large (max 25MB)",
    });
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 0,
      parts: 0,
    });
  });
});

describe("chunked uploads: complete", () => {
  it("names the missing parts and leaves the upload open", async () => {
    const f = fixture();
    const h = handlers(f);
    const data = bytes(2 * PART + 5);
    const session = await startUpload(h, f, data.length);
    await putPart(h, f, session.upload_id, 2, slicePart(data, 2));
    const params = { channelId: f.channel, uploadId: session.upload_id };
    const res = await h.complete(req("POST", "/x", f.user), params);
    expect(res.status).toBe(400);
    expect(await res.json()).toMatchObject({
      code: "UPLOAD_INCOMPLETE",
      missing: [1, 3],
    });
    // Still open: the rest can go up and complete.
    await putPart(h, f, session.upload_id, 1, slicePart(data, 1));
    await putPart(h, f, session.upload_id, 3, slicePart(data, 3));
    expect((await h.complete(req("POST", "/x", f.user), params)).status).toBe(
      201,
    );
  });

  it("enforces the limit again on the stored size", async () => {
    const f = fixture();
    let limit = 500 * MB;
    const h = handlers(f, { attachmentLimit: () => limit });
    const data = bytes(PART + 5);
    const session = await startUpload(h, f, data.length);
    await putPart(h, f, session.upload_id, 1, slicePart(data, 1));
    await putPart(h, f, session.upload_id, 2, slicePart(data, 2));
    limit = 5 * MB; // the plan shrank mid-upload
    const res = await h.complete(req("POST", "/x", f.user), {
      channelId: f.channel,
      uploadId: session.upload_id,
    });
    expect(res.status).toBe(413);
    expect(await res.json()).toMatchObject({
      error: "File too large (max 5MB)",
    });
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 0,
      parts: 0,
    });
  });

  it("completes once: a second complete is refused", async () => {
    const f = fixture();
    const h = handlers(f);
    const data = bytes(PART + 5);
    const session = await startUpload(h, f, data.length);
    await putPart(h, f, session.upload_id, 1, slicePart(data, 1));
    await putPart(h, f, session.upload_id, 2, slicePart(data, 2));
    const params = { channelId: f.channel, uploadId: session.upload_id };
    const [a, b] = await Promise.all([
      h.complete(req("POST", "/x", f.user), params),
      h.complete(req("POST", "/x", f.user), params),
    ]);
    expect([a.status, b.status].sort()).toEqual([201, 409].sort());
    const count = await db
      .prepare("SELECT COUNT(*) AS n FROM attachments WHERE user_id = ?")
      .bind(f.user)
      .first<{ n: number }>();
    expect(count?.n).toBe(1);
  });
});

describe("chunked uploads: abort and stale cleanup", () => {
  it("abort drops the rows and the R2 multipart", async () => {
    const f = fixture();
    const h = handlers(f);
    const session = await startUpload(h, f, PART + 5);
    await putPart(h, f, session.upload_id, 1, bytes(PART));
    const row = await db
      .prepare(
        "SELECT file_key, r2_upload_id FROM pending_uploads WHERE id = ?",
      )
      .bind(session.upload_id)
      .first<{ file_key: string; r2_upload_id: string }>();
    const params = { channelId: f.channel, uploadId: session.upload_id };
    const res = await h.abort(req("DELETE", "/x", f.user), params);
    expect(res.status).toBe(200);
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 0,
      parts: 0,
    });
    expect((await putPart(h, f, session.upload_id, 2, bytes(5))).status).toBe(
      404,
    );
    await expect(
      bucket
        .resumeMultipartUpload(row!.file_key, row!.r2_upload_id)
        .uploadPart(2, bytes(5)),
    ).rejects.toThrow();
  });

  it("the sweep aborts expired uploads and keeps live ones", async () => {
    const f = fixture();
    const t0 = Date.parse("2030-01-01T00:00:00Z");
    const h = handlers(f, { now: () => t0, ttlMs: 60 * 60 * 1000 });
    const live = handlers(f, { now: () => t0 + 2 * 60 * 60 * 1000 });
    const stale = await startUpload(h, f, PART + 5);
    await putPart(h, f, stale.upload_id, 1, bytes(PART));
    const fresh = await startUpload(live, f, PART + 5);

    const { aborted } = await cleanupStalePendingUploads(db, bucket, {
      now: t0 + 2 * 60 * 60 * 1000,
    });
    expect(aborted).toBeGreaterThanOrEqual(1);
    expect(await pendingRows(stale.upload_id)).toEqual({ upload: 0, parts: 0 });
    expect(await pendingRows(fresh.upload_id)).toEqual({ upload: 1, parts: 0 });
  });

  it("the sweep deletes an object a crashed complete left without an attachment", async () => {
    const f = fixture();
    const t0 = Date.parse("2031-01-01T00:00:00Z");
    const h = handlers(f, { now: () => t0, ttlMs: 1000 });
    const data = bytes(PART + 5);
    const session = await startUpload(h, f, data.length);
    await putPart(h, f, session.upload_id, 1, slicePart(data, 1));
    await putPart(h, f, session.upload_id, 2, slicePart(data, 2));
    // Simulate a complete that assembled the object and then died.
    const row = await db
      .prepare(
        "SELECT file_key, r2_upload_id FROM pending_uploads WHERE id = ?",
      )
      .bind(session.upload_id)
      .first<{ file_key: string; r2_upload_id: string }>();
    const { results: parts } = await db
      .prepare(
        "SELECT part_number, etag FROM pending_upload_parts WHERE upload_id = ? ORDER BY part_number",
      )
      .bind(session.upload_id)
      .all<{ part_number: number; etag: string }>();
    await bucket
      .resumeMultipartUpload(row!.file_key, row!.r2_upload_id)
      .complete(
        parts.map((p) => ({ partNumber: p.part_number, etag: p.etag })),
      );
    await db
      .prepare("UPDATE pending_uploads SET status = 'completing' WHERE id = ?")
      .bind(session.upload_id)
      .run();
    expect(await bucket.head(row!.file_key)).not.toBeNull();

    await cleanupStalePendingUploads(db, bucket, { now: t0 + 5000 });
    expect(await bucket.head(row!.file_key)).toBeNull();
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 0,
      parts: 0,
    });
  });
});

describe("chunked uploads: recovery", () => {
  it("finishes the attachment when R2 assembled the object but complete() threw", async () => {
    const f = fixture();
    const h = handlers(f, {
      bucket: hookedBucket({
        complete: async (real, parts) => {
          await real.complete(parts);
          throw new Error("complete timed out");
        },
      }),
    });
    const data = bytes(PART + 7);
    const session = await startUpload(h, f, data.length);
    await putPart(h, f, session.upload_id, 1, slicePart(data, 1));
    await putPart(h, f, session.upload_id, 2, slicePart(data, 2));
    const res = await h.complete(req("POST", "/x", f.user), {
      channelId: f.channel,
      uploadId: session.upload_id,
    });
    expect(res.status).toBe(201);
    const { id, file_url } = (await res.json()) as {
      id: string;
      file_url: string;
    };
    const object = await bucket.get(file_url.slice("/api/".length));
    expect(await sha256(await object!.arrayBuffer())).toBe(await sha256(data));
    const row = await db
      .prepare("SELECT size_bytes FROM attachments WHERE id = ?")
      .bind(id)
      .first<{ size_bytes: number }>();
    expect(row?.size_bytes).toBe(data.length);
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 0,
      parts: 0,
    });
  });

  it("releases the claim when complete() failed before assembling, so a retry works", async () => {
    const f = fixture();
    let fail = true;
    const h = handlers(f, {
      bucket: hookedBucket({
        complete: async (real, parts) => {
          if (fail) throw new Error("R2 unavailable");
          return real.complete(parts);
        },
      }),
    });
    const data = bytes(PART + 7);
    const session = await startUpload(h, f, data.length);
    await putPart(h, f, session.upload_id, 1, slicePart(data, 1));
    await putPart(h, f, session.upload_id, 2, slicePart(data, 2));
    const params = { channelId: f.channel, uploadId: session.upload_id };
    expect((await h.complete(req("POST", "/x", f.user), params)).status).toBe(
      502,
    );
    expect(await uploadStatus(session.upload_id)).toBe("open");
    fail = false;
    expect((await h.complete(req("POST", "/x", f.user), params)).status).toBe(
      201,
    );
  });

  it("the sweep deletes an assembled orphan behind an expired open row", async () => {
    // The pre-fix failure: complete() assembled, threw, and the row went back
    // to 'open'. Whatever the status, an expired row's object with no
    // attachment row is removed.
    const f = fixture();
    const t0 = Date.parse("2033-01-01T00:00:00Z");
    const h = handlers(f, { now: () => t0, ttlMs: 1000 });
    const data = bytes(PART + 5);
    const session = await startUpload(h, f, data.length);
    await putPart(h, f, session.upload_id, 1, slicePart(data, 1));
    await putPart(h, f, session.upload_id, 2, slicePart(data, 2));
    const row = await db
      .prepare(
        "SELECT file_key, r2_upload_id FROM pending_uploads WHERE id = ?",
      )
      .bind(session.upload_id)
      .first<{ file_key: string; r2_upload_id: string }>();
    const { results: parts } = await db
      .prepare(
        "SELECT part_number, etag FROM pending_upload_parts WHERE upload_id = ? ORDER BY part_number",
      )
      .bind(session.upload_id)
      .all<{ part_number: number; etag: string }>();
    await bucket
      .resumeMultipartUpload(row!.file_key, row!.r2_upload_id)
      .complete(
        parts.map((p) => ({ partNumber: p.part_number, etag: p.etag })),
      );
    expect(await uploadStatus(session.upload_id)).toBe("open");

    await cleanupStalePendingUploads(db, bucket, { now: t0 + 5000 });
    expect(await bucket.head(row!.file_key)).toBeNull();
    expect(await pendingRows(session.upload_id)).toEqual({
      upload: 0,
      parts: 0,
    });
  });

  it("the sweep expires idle open uploads after an hour and keeps active ones", async () => {
    const f = fixture();
    const t0 = Date.parse("2034-01-01T00:00:00Z");
    let clock = t0;
    const h = handlers(f, { now: () => clock });
    const idle = await startUpload(h, f, PART + 5);
    const active = await startUpload(h, f, PART + 5);
    clock = t0 + 30 * 60 * 1000;
    expect((await putPart(h, f, active.upload_id, 1, bytes(PART))).status).toBe(
      200,
    );

    await cleanupStalePendingUploads(db, bucket, { now: t0 + 61 * 60 * 1000 });
    expect(await pendingRows(idle.upload_id)).toEqual({ upload: 0, parts: 0 });
    expect(await pendingRows(active.upload_id)).toEqual({
      upload: 1,
      parts: 1,
    });

    // An abandoned tab no longer blocks the user: a later create just works.
    clock = t0 + 62 * 60 * 1000;
    const next = await h.create(
      req(
        "POST",
        "/x",
        f.user,
        JSON.stringify({ filename: "n", size: PART + 1 }),
      ),
      { channelId: f.channel },
    );
    expect(next.status).toBe(201);
  });

  it("a part that lands after complete claimed the upload answers 409, not 404", async () => {
    const f = fixture();
    let uploadId = "";
    const h = handlers(f, {
      bucket: hookedBucket({
        // A complete claims the row while this part is being stored.
        afterPart: async () => {
          await db
            .prepare(
              "UPDATE pending_uploads SET status = 'completing' WHERE id = ?",
            )
            .bind(uploadId)
            .run();
        },
      }),
    });
    const session = await startUpload(h, f, PART + 5);
    uploadId = session.upload_id;
    const res = await putPart(h, f, session.upload_id, 1, bytes(PART));
    expect(res.status).toBe(409);
    expect(await res.json()).toMatchObject({ code: "UPLOAD_COMPLETING" });
  });
});
