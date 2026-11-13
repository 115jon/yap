/**
 * The `customer_id` KLIPY gets for a signed-in user: a pseudonym, never the
 * Yap user id. KLIPY uses it to tell one searcher from another (recents,
 * personalised results), so it has to be stable per user, but KLIPY has no
 * use for who that user is on Yap.
 *
 * HMAC-SHA-256 over the user id, keyed with the server's realtime ticket
 * secret under its own label (so the key is never used for anything else),
 * cut to 32 hex characters. Without the secret, which only local dev lacks,
 * there is no pseudonym and the request goes without a customer_id.
 */

const LABEL = "yap/klipy-customer-id/v1:";

const encoder = new TextEncoder();

export async function klipyCustomerId(
  userId: string,
  secret: string | undefined,
): Promise<string | undefined> {
  if (!userId || !secret) return undefined;
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = new Uint8Array(
    await crypto.subtle.sign("HMAC", key, encoder.encode(LABEL + userId)),
  );
  let hex = "";
  for (const byte of mac.subarray(0, 16)) {
    hex += byte.toString(16).padStart(2, "0");
  }
  return hex;
}
