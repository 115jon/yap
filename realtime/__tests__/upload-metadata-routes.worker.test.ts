// The public-display upload routes (avatar, banner, nameplate, server icon,
// camera background, voice status media) against a real D1 and R2: a file
// whose metadata scan comes back unchecked is refused with a 415 and never
// reaches R2. Kova is faked at its get-session call; the voice-session gate
// (a live call in the meeting room) is stubbed, the rest is real.

import { env } from "cloudflare:workers";
import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";

import schema from "../../d1_schema.sql?raw";
import { setTestRequestHeaders } from "../test-shims/react-start-server";
import {
  box,
  concat,
  pngWith,
  text,
} from "../../src/lib/__tests__/fixtures/metadata-files";
import { PERMISSIONS } from "../../src/lib/permissions";
import { DEFAULT_KOVA_AUTH_URL } from "../../src/lib/site-defaults";
import { invalidateSuspensionCache } from "../../src/lib/suspension";
import {
  LOCATION_TRACK_MESSAGE,
  UNCHECKED_IMAGE_MESSAGE,
  UNCHECKED_UPLOAD_CODE,
  UNSUPPORTED_IMAGE_MESSAGE,
} from "../../src/lib/upload-metadata/strip-upload";

vi.mock("../../src/lib/api-helpers", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../src/lib/api-helpers")>()),
  requireActiveVoiceChannelSession: async () => ({
    sessionId: "voice-session",
    exactSessionMatched: true,
  }),
}));

// Route modules load at runtime by path: their createFileRoute types need
// the app's generated route tree, which this tsconfig does not include.
type RouteModule = { Route: { options: unknown } };
const loadRoute = (path: string) =>
  import(/* @vite-ignore */ path) as Promise<RouteModule>;

const AvatarRoute = (await loadRoute("../../src/routes/api/avatar-upload"))
  .Route;
const ProfileAssetRoute = (
  await loadRoute("../../src/routes/api/profile-assets/manage")
).Route;
const ServerIconRoute = (
  await loadRoute("../../src/routes/api/servers/icon-upload")
).Route;
const CameraBackgroundRoute = (
  await loadRoute("../../src/routes/api/camera-backgrounds")
).Route;
const VoiceStatusMediaRoute = (
  await loadRoute("../../src/routes/api/channels/$id/voice-status-media")
).Route;

const db = env.DB as unknown as D1Database;
const bucket = env.BUCKET as unknown as R2Bucket;
const BASE = "https://yap.test";
const USER = "upload-user";
const TOKEN = "upload-token";
const SERVER = "upload-server";
const VOICE_CHANNEL = "upload-voice";

type Handler = (args: {
  request: Request;
  params?: Record<string, string>;
}) => Promise<Response>;

function handler(route: { options: unknown }): Handler {
  const handlers = (
    route.options as { server: { handlers: Record<string, Handler> } }
  ).server.handlers;
  const fn = handlers.POST;
  if (!fn) throw new Error("no POST handler");
  return fn;
}

const realFetch = globalThis.fetch;

function fakeKova() {
  return vi
    .spyOn(globalThis, "fetch")
    .mockImplementation(async (input, init) => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : input.url;
      if (url.startsWith(`${DEFAULT_KOVA_AUTH_URL}/api/auth/get-session`)) {
        const headers = new Headers(
          init?.headers ??
            (input instanceof Request ? input.headers : undefined),
        );
        return headers.get("authorization") === `Bearer ${TOKEN}`
          ? Response.json({
              user: { id: USER, username: "uploader", name: "uploader" },
              session: { id: `session-${USER}` },
            })
          : Response.json({ error: "unauthorized" }, { status: 401 });
      }
      return realFetch(input as RequestInfo, init);
    });
}

async function run(sql: string, ...binds: unknown[]) {
  await db
    .prepare(sql)
    .bind(...binds)
    .run();
}

beforeAll(async () => {
  const statements = schema
    .split("\n")
    .map((line) => line.replace(/--.*$/, ""))
    .join("\n")
    .split(";")
    .map((statement) => statement.trim())
    .filter(Boolean);
  for (const statement of statements) {
    await db.prepare(statement).run();
  }
  await run(
    `INSERT OR IGNORE INTO users (id, username, display_name, created_at) VALUES (?, 'uploader', 'uploader', ?)`,
    USER,
    new Date().toISOString(),
  );
  await run(
    `INSERT OR IGNORE INTO servers (id, name, owner_id) VALUES (?, 'Uploads', ?)`,
    SERVER,
    USER,
  );
  await run(
    `INSERT OR IGNORE INTO server_members (server_id, user_id) VALUES (?, ?)`,
    SERVER,
    USER,
  );
  await run(
    `INSERT OR IGNORE INTO channels (id, server_id, name, channel_type) VALUES (?, ?, 'Voice', 'voice')`,
    VOICE_CHANNEL,
    SERVER,
  );
  // @everyone may see and join the voice channel.
  await run(
    `INSERT OR IGNORE INTO roles (id, server_id, name, permissions, position, is_default) VALUES (?, ?, '@everyone', ?, 0, 1)`,
    SERVER,
    SERVER,
    PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.CONNECT,
  );
  await run(
    `INSERT OR IGNORE INTO member_roles (server_id, user_id, role_id) VALUES (?, ?, ?)`,
    SERVER,
    USER,
    SERVER,
  );
});

beforeEach(() => {
  fakeKova();
  invalidateSuspensionCache();
  // The server icon route reads the current request's headers.
  setTestRequestHeaders({ authorization: `Bearer ${TOKEN}` });
});

afterEach(() => {
  vi.restoreAllMocks();
  setTestRequestHeaders(null);
});

// ── Files ──────────────────────────────────────────────────────────────────

/** A PNG signature and nothing a decoder can walk: the scan can't finish. */
function brokenPng(): Uint8Array {
  const bytes = new Uint8Array(64);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  return bytes;
}

/** An AVIF header (validateImageBuffer takes it; the scanner can't clean it). */
function avif(): Uint8Array {
  return concat(
    box("ftyp", text("avif"), [0, 0, 0, 0], text("mif1"), text("avif")),
    box("meta", [0, 0, 0, 0]),
  );
}

/** An MP4 with a GoPro GPMF track: a GPS fix per frame. */
function goproMp4(): Uint8Array {
  const u32 = (n: number) => [
    n >>> 24,
    (n >>> 16) & 255,
    (n >>> 8) & 255,
    n & 255,
  ];
  return concat(
    box("ftyp", text("mp41"), [0, 0, 0, 0], text("mp41")),
    box(
      "moov",
      box(
        "trak",
        box(
          "mdia",
          box(
            "minf",
            box(
              "stbl",
              box("stsd", u32(0), u32(1), box("gpmd", new Uint8Array(16))),
            ),
          ),
        ),
      ),
    ),
    box("mdat", new Uint8Array(32)),
  );
}

function form(
  name: string,
  bytes: Uint8Array,
  type: string,
  fields: Record<string, string> = {},
): FormData {
  const data = new FormData();
  for (const [key, value] of Object.entries(fields)) data.append(key, value);
  data.append("file", new File([new Uint8Array(bytes)], name, { type }));
  return data;
}

function post(path: string, body: FormData): Request {
  return new Request(`${BASE}${path}`, {
    method: "POST",
    headers: { authorization: `Bearer ${TOKEN}`, "cf-ray": "test-ray-1" },
    body,
  });
}

async function storedUnder(prefix: string): Promise<string[]> {
  const listed = await bucket.list({ prefix });
  return listed.objects.map((object) => object.key);
}

async function expectRefused(response: Response, message: string) {
  // One assertion, so a wrong status shows the body that came with it.
  expect({
    status: response.status,
    body: await response.json(),
  }).toMatchObject({
    status: 415,
    body: { error: message, code: UNCHECKED_UPLOAD_CODE },
  });
}

// ── Routes ─────────────────────────────────────────────────────────────────

const IMAGE_CASES = [
  ["broken.png", brokenPng, "image/png", UNCHECKED_IMAGE_MESSAGE],
  ["photo.avif", avif, "image/avif", UNSUPPORTED_IMAGE_MESSAGE],
] as const;

describe("unchecked uploads on public routes", () => {
  it("avatar: refuses and stores nothing", async () => {
    const before = await storedUnder(`avatars/${USER}/`);
    for (const [name, file, type, message] of IMAGE_CASES) {
      const response = await handler(AvatarRoute)({
        request: post("/api/avatar-upload", form(name, file(), type)),
      });
      await expectRefused(response, message);
    }
    expect(await storedUnder(`avatars/${USER}/`)).toEqual(before);
  });

  it("banner: refuses and stores nothing", async () => {
    const before = await storedUnder("profile-assets/banner/");
    for (const [name, file, type, message] of IMAGE_CASES) {
      const response = await handler(ProfileAssetRoute)({
        request: post(
          "/api/profile-assets/manage",
          form(name, file(), type, { kind: "banner" }),
        ),
      });
      await expectRefused(response, message);
    }
    expect(await storedUnder("profile-assets/banner/")).toEqual(before);
  });

  it("nameplate: refuses images and a video with a GPS track", async () => {
    const before = await storedUnder("profile-assets/nameplate/");
    for (const [name, file, type, message] of [
      ...IMAGE_CASES,
      ["ride.mp4", goproMp4, "video/mp4", LOCATION_TRACK_MESSAGE],
    ] as const) {
      const response = await handler(ProfileAssetRoute)({
        request: post(
          "/api/profile-assets/manage",
          form(name, file(), type, { kind: "nameplate" }),
        ),
      });
      await expectRefused(response, message);
    }
    expect(await storedUnder("profile-assets/nameplate/")).toEqual(before);
  });

  it("server icon: refuses and stores nothing", async () => {
    const before = await storedUnder("server-icons/");
    for (const [name, file, type, message] of IMAGE_CASES) {
      const response = await handler(ServerIconRoute)({
        request: post("/api/servers/icon-upload", form(name, file(), type)),
      });
      await expectRefused(response, message);
    }
    expect(await storedUnder("server-icons/")).toEqual(before);
  });

  it("server icon: still stores a clean PNG", async () => {
    const response = await handler(ServerIconRoute)({
      request: post(
        "/api/servers/icon-upload",
        form("icon.png", pngWith(), "image/png"),
      ),
    });
    expect(response.status).toBe(201);
    const { key } = (await response.json()) as { key: string };
    expect(await bucket.head(key)).not.toBeNull();
  });

  it("camera background: refuses and stores nothing", async () => {
    const before = await storedUnder("camera-backgrounds/");
    for (const [name, file, type, message] of IMAGE_CASES) {
      const response = await handler(CameraBackgroundRoute)({
        request: post("/api/camera-backgrounds", form(name, file(), type)),
      });
      await expectRefused(response, message);
    }
    expect(await storedUnder("camera-backgrounds/")).toEqual(before);
  });

  it("voice status media: refuses images and a video with a GPS track", async () => {
    const before = await storedUnder("voice-status-media/");
    for (const [name, file, type, message] of [
      ["broken.png", brokenPng, "image/png", UNCHECKED_IMAGE_MESSAGE],
      // Declared as a PNG, so only the scan sees what it is.
      ["photo.png", avif, "image/png", UNSUPPORTED_IMAGE_MESSAGE],
      ["ride.mp4", goproMp4, "video/mp4", LOCATION_TRACK_MESSAGE],
    ] as const) {
      const response = await handler(VoiceStatusMediaRoute)({
        request: post(
          `/api/channels/${VOICE_CHANNEL}/voice-status-media`,
          form(name, file(), type),
        ),
        params: { id: VOICE_CHANNEL },
      });
      await expectRefused(response, message);
    }
    expect(await storedUnder("voice-status-media/")).toEqual(before);
  });
});
