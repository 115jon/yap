// "Who can DM you": the setting's values and the copy for its refusals,
// shared by the server (src/services/dm-permission.service.ts enforces it)
// and the clients. native/rust/crates/yap-core/src/dm_permission.rs mirrors
// this file.
//
// - everyone (the default): anyone can start a DM with you.
// - friends_and_servers: only friends and people who share a server with
//   you can.
// - nobody: no one can start a new DM with you.
//
// The setting gates new conversations: a DM you have written in (a message,
// or a call you placed) stays open whatever it says. A block either way
// closes a DM under every setting, and server admins get no bypass.

export const DM_PERMISSIONS = [
  "everyone",
  "friends_and_servers",
  "nobody",
] as const;

export type DmPermission = (typeof DM_PERMISSIONS)[number];

/** What every account had before the setting existed. */
export const DEFAULT_DM_PERMISSION: DmPermission = "everyone";

/** The API's error code for a refusal over the recipient's setting. */
export const DM_NOT_ACCEPTING_CODE = "NOT_ACCEPTING_DMS";

/** The gateway's CALL_RING_STOP reason for a ring refused by the setting. */
export const RING_NOT_ACCEPTING_REASON = "not_accepting";

export function isDmPermission(value: unknown): value is DmPermission {
  return (
    typeof value === "string" &&
    (DM_PERMISSIONS as readonly string[]).includes(value)
  );
}

/** A stored value, or the default for anything else. */
export function dmPermissionOf(value: unknown): DmPermission {
  return isDmPermission(value) ? value : DEFAULT_DM_PERMISSION;
}

/** The settings picker's options, in order. */
export const DM_PERMISSION_OPTIONS: ReadonlyArray<{
  value: DmPermission;
  label: string;
  description: string;
}> = [
  {
    value: "everyone",
    label: "Everyone",
    description: "Anyone on Yap can start a DM with you.",
  },
  {
    value: "friends_and_servers",
    label: "Friends & servers",
    description:
      "Only your friends and people who share a server with you can start a DM.",
  },
  {
    value: "nobody",
    label: "No one",
    description:
      "No one can start a new DM with you. Conversations you've written in stay open.",
  },
];

/** The name a refusal uses: display name, else username. */
function shownName(name: string | null | undefined): string {
  const trimmed = name?.trim();
  return trimmed ? trimmed : "This person";
}

/** "Alex isn't accepting messages from you." */
export function dmNotAcceptingMessage(name: string | null | undefined): string {
  return `${shownName(name)} isn't accepting messages from you.`;
}

/** "Alex isn't accepting calls from you." */
export function dmNotAcceptingCall(name: string | null | undefined): string {
  return `${shownName(name)} isn't accepting calls from you.`;
}

/** The server's sentence for a refusal over the setting, or null. */
export function dmNotAcceptingErrorMessage(error: unknown): string | null {
  const code = (error as { code?: unknown } | null)?.code;
  if (code !== DM_NOT_ACCEPTING_CODE) return null;
  const message = (error as { message?: unknown } | null)?.message;
  return typeof message === "string" && message.trim()
    ? message
    : dmNotAcceptingMessage(null);
}
