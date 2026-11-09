/**
 * Steam: "Sign in through Steam" (OpenID 2.0). No API key: ownership is
 * proven by Steam's own check_authentication answer, and the persona name
 * and avatar come from the public profile XML.
 *
 * The callback is trusted only when every one of these holds:
 *  - it is a positive assertion (`id_res`) in the OpenID 2.0 namespace, with
 *    no openid.* field given twice;
 *  - `op_endpoint` is Steam's login endpoint and `return_to` is exactly the
 *    URL this flow sent (it carries the one-time state);
 *  - `claimed_id` and `identity` are the same steamcommunity.com id URL;
 *  - the signature covers op_endpoint, claimed_id, identity, return_to,
 *    response_nonce and assoc_handle, and the nonce is recent;
 *  - Steam answers `is_valid:true` when the fields are posted back to the
 *    fixed endpoint below (never to the endpoint the response names).
 */

import { getBotUserAgent } from "@/lib/platform";

import type {
  ConnectionProviderModule,
  VerifiedAccount,
  VerifyResult,
} from "./types";

export const STEAM_OPENID_ENDPOINT = "https://steamcommunity.com/openid/login";
const OPENID_NS = "http://specs.openid.net/auth/2.0";
const IDENTIFIER_SELECT = "http://specs.openid.net/auth/2.0/identifier_select";
const CLAIMED_ID_RE = /^https:\/\/steamcommunity\.com\/openid\/id\/(\d{17})$/;
/** SteamID64 of account id 0 in the public universe (individual accounts). */
const STEAM_ID64_BASE = 76561197960265728n;
const REQUIRED_SIGNED = [
  "op_endpoint",
  "claimed_id",
  "identity",
  "return_to",
  "response_nonce",
  "assoc_handle",
];
/** How old (or how far ahead) a response_nonce may be. */
const NONCE_MAX_AGE_MS = 10 * 60_000;
const NONCE_MAX_SKEW_MS = 5 * 60_000;
const CHECK_TIMEOUT_MS = 8_000;
const PROFILE_TIMEOUT_MS = 5_000;
const PROFILE_MAX_BYTES = 256 * 1024;
const NAME_MAX = 64;
/** Steam's avatar CDNs; any other avatar URL is dropped. */
const AVATAR_HOSTS = new Set([
  "avatars.steamstatic.com",
  "avatars.akamai.steamstatic.com",
  "avatars.fastly.steamstatic.com",
  "avatars.cloudflare.steamstatic.com",
]);

export function isSteamAvatarUrl(raw: string): boolean {
  try {
    const url = new URL(raw);
    return (
      url.protocol === "https:" &&
      AVATAR_HOSTS.has(url.hostname) &&
      !url.username &&
      !url.password &&
      !url.port
    );
  } catch {
    return false;
  }
}

/**
 * The 32-bit account id of a SteamID64 (what Deadlock's APIs take), or null
 * when the id is not an individual account in the public universe.
 */
export function steamAccountId(id64: string): number | null {
  if (!/^\d{17}$/.test(id64)) return null;
  const accountId = BigInt(id64) - STEAM_ID64_BASE;
  if (accountId <= 0n || accountId > 0xffffffffn) return null;
  return Number(accountId);
}

/** The SteamID64 in a claimed_id, or null for anything but Steam's id URL. */
export function parseSteamClaimedId(claimedId: string): string | null {
  const match = CLAIMED_ID_RE.exec(claimedId);
  if (!match) return null;
  return steamAccountId(match[1]!) === null ? null : match[1]!;
}

export function steamProfileUrl(id64: string): string {
  return `https://steamcommunity.com/profiles/${id64}`;
}

/** The openid.* fields once each, or null when any repeats. */
function openIdFields(url: URL): Map<string, string> | null {
  const fields = new Map<string, string>();
  for (const [key, value] of url.searchParams) {
    if (!key.startsWith("openid.")) continue;
    if (fields.has(key)) return null;
    fields.set(key, value);
  }
  return fields;
}

/** The time at the start of an OpenID response_nonce ("2026-11-09T12:00:00Z…"). */
function nonceTime(nonce: string): number | null {
  const match = /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z)/.exec(nonce);
  if (!match) return null;
  const time = Date.parse(match[1]!);
  return Number.isFinite(time) ? time : null;
}

/** Posts the assertion back to Steam. true / false, or null if Steam can't say. */
async function checkAuthentication(
  fields: Map<string, string>,
): Promise<boolean | null> {
  const body = new URLSearchParams();
  for (const [key, value] of fields) body.set(key, value);
  body.set("openid.mode", "check_authentication");
  let response: Response;
  try {
    response = await fetch(STEAM_OPENID_ENDPOINT, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "text/plain",
        "User-Agent": getBotUserAgent(),
      },
      body: body.toString(),
      redirect: "manual",
      signal: AbortSignal.timeout(CHECK_TIMEOUT_MS),
    });
  } catch {
    return null;
  }
  if (response.status !== 200) return null;
  let text: string;
  try {
    text = await response.text();
  } catch {
    return null;
  }
  // Key-value form: "ns:…\nis_valid:true\n". Exactly one is_valid line.
  const values = text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith("is_valid:"))
    .map((line) => line.slice("is_valid:".length));
  if (values.length !== 1) return false;
  return values[0] === "true";
}

const XML_ENTITIES: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
};

function decodeXmlText(text: string): string {
  return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, entity) => {
    const name = String(entity).toLowerCase();
    if (name.startsWith("#x")) {
      const code = Number.parseInt(name.slice(2), 16);
      return Number.isFinite(code) && code <= 0x10ffff
        ? String.fromCodePoint(code)
        : "";
    }
    if (name.startsWith("#")) {
      const code = Number.parseInt(name.slice(1), 10);
      return Number.isFinite(code) && code <= 0x10ffff
        ? String.fromCodePoint(code)
        : "";
    }
    return XML_ENTITIES[name] ?? whole;
  });
}

/** One top-level element's text, CDATA or plain. */
export function readXmlField(xml: string, tag: string): string | null {
  const cdata = new RegExp(
    `<${tag}>\\s*<!\\[CDATA\\[([\\s\\S]*?)\\]\\]>\\s*</${tag}>`,
  ).exec(xml);
  if (cdata) return cdata[1]!;
  const plain = new RegExp(`<${tag}>([^<]*)</${tag}>`).exec(xml);
  return plain ? decodeXmlText(plain[1]!) : null;
}

/** A persona name fit to print: no control characters, one line, ≤ 64. */
export function cleanPersonaName(raw: string | null): string | null {
  if (!raw) return null;
  // eslint-disable-next-line no-control-regex
  const flat = raw.replace(/[\u0000-\u001f\u007f-\u009f​-‏‪-‮⁦-⁩]/g, " ");
  const name = Array.from(flat.replace(/\s+/g, " ").trim())
    .slice(0, NAME_MAX)
    .join("")
    .trim();
  return name || null;
}

/** Persona name and avatar from the public profile XML; nulls if unreadable. */
export async function fetchSteamProfile(
  id64: string,
): Promise<{ name: string | null; avatarUrl: string | null }> {
  const none = { name: null, avatarUrl: null };
  let response: Response;
  try {
    response = await fetch(`${steamProfileUrl(id64)}?xml=1`, {
      headers: {
        Accept: "application/xml, text/xml",
        "User-Agent": getBotUserAgent(),
      },
      redirect: "follow",
      signal: AbortSignal.timeout(PROFILE_TIMEOUT_MS),
    });
  } catch {
    return none;
  }
  if (!response.ok) return none;
  if (response.url && new URL(response.url).hostname !== "steamcommunity.com") {
    return none;
  }
  const declared = Number(response.headers.get("Content-Length"));
  if (Number.isFinite(declared) && declared > PROFILE_MAX_BYTES) return none;
  let xml: string;
  try {
    xml = await response.text();
  } catch {
    return none;
  }
  if (xml.length > PROFILE_MAX_BYTES) return none;
  // The XML must be about this account.
  if (readXmlField(xml, "steamID64")?.trim() !== id64) return none;
  const avatar = readXmlField(xml, "avatarFull")?.trim() ?? null;
  return {
    name: cleanPersonaName(readXmlField(xml, "steamID")),
    avatarUrl: avatar && isSteamAvatarUrl(avatar) ? avatar : null,
  };
}

export const steamProvider: ConnectionProviderModule = {
  id: "steam",

  authorizationUrl({ returnTo, realm }) {
    const params = new URLSearchParams({
      "openid.ns": OPENID_NS,
      "openid.mode": "checkid_setup",
      "openid.return_to": returnTo,
      "openid.realm": realm,
      "openid.identity": IDENTIFIER_SELECT,
      "openid.claimed_id": IDENTIFIER_SELECT,
    });
    return `${STEAM_OPENID_ENDPOINT}?${params.toString()}`;
  },

  async verifyCallback({
    url,
    expectedReturnTo,
    now = Date.now(),
  }): Promise<VerifyResult> {
    const fields = openIdFields(url);
    if (!fields) return { ok: false, reason: "invalid", detail: "repeated" };
    const field = (name: string) => fields.get(`openid.${name}`) ?? "";

    const mode = field("mode");
    if (mode === "cancel") return { ok: false, reason: "cancelled" };
    if (mode !== "id_res")
      return { ok: false, reason: "invalid", detail: "mode" };
    if (field("ns") !== OPENID_NS) {
      return { ok: false, reason: "invalid", detail: "ns" };
    }
    if (field("op_endpoint") !== STEAM_OPENID_ENDPOINT) {
      return { ok: false, reason: "invalid", detail: "op_endpoint" };
    }
    if (field("return_to") !== expectedReturnTo) {
      return { ok: false, reason: "invalid", detail: "return_to" };
    }
    const claimedId = field("claimed_id");
    if (field("identity") !== claimedId) {
      return { ok: false, reason: "invalid", detail: "identity" };
    }
    const id64 = parseSteamClaimedId(claimedId);
    if (!id64) return { ok: false, reason: "invalid", detail: "claimed_id" };
    if (!field("sig") || !field("assoc_handle")) {
      return { ok: false, reason: "invalid", detail: "sig" };
    }
    const signed = new Set(field("signed").split(","));
    if (!REQUIRED_SIGNED.every((name) => signed.has(name))) {
      return { ok: false, reason: "invalid", detail: "signed" };
    }
    const issued = nonceTime(field("response_nonce"));
    if (
      issued === null ||
      now - issued > NONCE_MAX_AGE_MS ||
      issued - now > NONCE_MAX_SKEW_MS
    ) {
      return { ok: false, reason: "invalid", detail: "nonce" };
    }

    const valid = await checkAuthentication(fields);
    if (valid === null) return { ok: false, reason: "unavailable" };
    if (!valid) return { ok: false, reason: "invalid", detail: "is_valid" };

    const accountId = steamAccountId(id64)!;
    const profile = await fetchSteamProfile(id64);
    const account: VerifiedAccount = {
      externalId: id64,
      // Without the XML the profile URL is all there is; it already shows the id.
      displayName: profile.name ?? id64,
      profileUrl: steamProfileUrl(id64),
      avatarUrl: profile.avatarUrl,
      metadata: { steam_id64: id64, account_id: accountId },
    };
    return { ok: true, account };
  },
};
