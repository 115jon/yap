import { MAX_PEER_VOLUME } from "@/lib/voice/peer-volume";

// The boost slider's rules (ContextMenuItems `Slider` with `boost`), matching
// the native client's VolumeSlider.qml `boost: true`.

/** A boost slider's range: per-user and per-stream volume go to 400 %. */
export const BOOST_MAX = MAX_PEER_VOLUME;
/** Past this a boost slider lights the warning ink (DESIGN.md: amber is the warning light). */
export const BOOST_HOT = 200;
/** Within this many pixels of 100 % a drag lands on it. */
export const BOOST_SNAP_PX = 6;
const BOOST_STEP = 5;
const BOOST_PAGE_STEP = 25;

/**
 * Where a boost slider's key goes from `value`: to the next whole step in
 * that direction (103 goes to 105 or 100), PageUp/PageDown by 25, Home/End
 * to the ends. `null` for keys the slider leaves alone.
 */
export function boostKeyTarget(
  key: string,
  value: number,
  max = BOOST_MAX,
): number | null {
  const up = (step: number) => (Math.floor(value / step + 1e-6) + 1) * step;
  const down = (step: number) => (Math.ceil(value / step - 1e-6) - 1) * step;
  let next: number;
  switch (key) {
    case "ArrowRight":
    case "ArrowUp":
      next = up(BOOST_STEP);
      break;
    case "ArrowLeft":
    case "ArrowDown":
      next = down(BOOST_STEP);
      break;
    case "PageUp":
      next = up(BOOST_PAGE_STEP);
      break;
    case "PageDown":
      next = down(BOOST_PAGE_STEP);
      break;
    case "Home":
      next = 0;
      break;
    case "End":
      next = max;
      break;
    default:
      return null;
  }
  return Math.max(0, Math.min(max, next));
}

/** A dragged boost value lands on 100 % within `BOOST_SNAP_PX` of it. */
export function snapBoost(
  value: number,
  trackWidth: number,
  max = BOOST_MAX,
): number {
  if (trackWidth <= 0) return value;
  const perPixel = max / trackWidth;
  return Math.abs(value - 100) <= BOOST_SNAP_PX * perPixel ? 100 : value;
}
