/**
 * Per-user volume and per-user stream volume, in percent: 0 is muted, 100 is
 * as sent, 400 is the ceiling (the native client's `MAX_PEER_VOLUME`). Past
 * 100 % the playback graph puts that person behind their own limiter
 * (`createBoostLimiter` in audio-pipeline.ts), so a boost gets louder
 * without clipping or pumping everyone else.
 */
export const MAX_PEER_VOLUME = 400;
export const DEFAULT_PEER_VOLUME = 100;

/** The same ceiling as a gain (`percent / 100`). */
export const MAX_PEER_GAIN = MAX_PEER_VOLUME / 100;

/**
 * A stored or entered per-user volume, kept in 0..400 %. Values saved by older
 * releases (0..200 %) are already in range and come back unchanged; anything
 * that is not a finite number becomes 100 %.
 */
export function clampPeerVolume(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_PEER_VOLUME;
  return Math.max(0, Math.min(value, MAX_PEER_VOLUME));
}
