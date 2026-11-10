/**
 * The desktop native handoff switch: whether the last Tauri release (3.14)
 * installs the native app and hands over to it, and for what share of
 * installs. Pure (no server imports), so the main Worker, the admin console
 * and the tests read one definition.
 *
 * Stored in D1 `site_settings` under key `nativeHandoff`
 * (services/site-settings-store.ts), written by the admin Worker. Unlike the
 * kill switches it defaults to off: a missing or unreadable row means no
 * handoff. The Tauri app (desktop/src-tauri/src/native_handoff) reads it from
 * GET /api/desktop/native-handoff and also hands off only when it gets a
 * readable answer.
 */

export interface NativeHandoffConfig {
  /** Off: no Tauri install hands off, whatever the rollout says. */
  enabled: boolean;
  /** 0–100: installs whose stable bucket (0–99) is below it hand off. */
  rolloutPercent: number;
}

export const DEFAULT_NATIVE_HANDOFF: NativeHandoffConfig = Object.freeze({
  enabled: false,
  rolloutPercent: 0,
});

/** A whole percent 0–100, or null for anything else. */
export function parseRolloutPercent(value: unknown): number | null {
  if (typeof value !== "number" || !Number.isInteger(value)) return null;
  return value >= 0 && value <= 100 ? value : null;
}

/** Reads a stored value; anything unreadable is the default (off, 0 %). */
export function parseNativeHandoff(value: unknown): NativeHandoffConfig {
  if (!value || typeof value !== "object") return { ...DEFAULT_NATIVE_HANDOFF };
  const raw = value as Record<string, unknown>;
  const percent = parseRolloutPercent(raw.rolloutPercent);
  if (typeof raw.enabled !== "boolean" || percent === null) {
    return { ...DEFAULT_NATIVE_HANDOFF };
  }
  return { enabled: raw.enabled, rolloutPercent: percent };
}
