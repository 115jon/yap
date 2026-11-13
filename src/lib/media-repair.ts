/**
 * Repairs private media that failed to load because its URL was signed late
 * or not at all: rendered before the media grant arrived, or kept on screen
 * past its capability's expiry (lib/media-grant). One capture-phase `error`
 * listener on the document covers every `<img>`, `<video>`, `<audio>` and
 * `<source>`, so the ~55 components that render media need no changes.
 *
 * The listener holds the error back from the element's own handlers while it
 * signs the URL again. When that gives nothing new, it re-dispatches the
 * error, so components still show their fallback.
 */
import { getPublicApiUrl } from "@/lib/platform";
import {
  isCapProtectedMediaPath,
  isMediaCap,
  MEDIA_CAP_PARAM,
  stripMediaCredentials,
} from "@/lib/media-capability";
import {
  ensureMediaGrant,
  forgetSignedMediaUrl,
  getMediaGrant,
  signMediaUrl,
} from "@/lib/media-grant";

type MediaElement = HTMLImageElement | HTMLMediaElement | HTMLSourceElement;

/** The src each element was last given here: one repair per src. */
const repaired = new WeakMap<Element, string>();
/** Errors this module re-dispatched: they pass straight through. */
const passthrough = new WeakSet<Event>();

function isMediaElement(target: EventTarget | null): target is MediaElement {
  return (
    target instanceof HTMLImageElement ||
    target instanceof HTMLMediaElement ||
    target instanceof HTMLSourceElement
  );
}

function isOwnOrigin(url: URL): boolean {
  if (url.origin === window.location.origin) return true;
  try {
    return url.origin === new URL(getPublicApiUrl()).origin;
  } catch {
    return false;
  }
}

/** A private media URL this module may sign again (not an issued `sbm1` cap). */
export function repairableMediaUrl(src: string): URL | null {
  let url: URL;
  try {
    url = new URL(src, window.location.href);
  } catch {
    return null;
  }
  if (!isOwnOrigin(url) || !isCapProtectedMediaPath(url.pathname)) return null;
  const cap = url.searchParams.get(MEDIA_CAP_PARAM);
  if (cap !== null && !isMediaCap(cap)) return null;
  return url;
}

function giveUp(element: MediaElement): void {
  const event = new Event("error");
  passthrough.add(event);
  element.dispatchEvent(event);
}

async function repair(
  element: MediaElement,
  failed: URL,
  failedSrc: string,
): Promise<void> {
  const unsigned = new URL(failed.toString());
  stripMediaCredentials(unsigned);
  forgetSignedMediaUrl(unsigned);
  if (failed.searchParams.has(MEDIA_CAP_PARAM)) {
    // Signed and still refused: the grant may be dead (sleep, a wrong clock).
    await ensureMediaGrant({ force: true });
  } else if (!getMediaGrant()) {
    await ensureMediaGrant();
  }

  const signed = signMediaUrl(unsigned);
  if (
    !signed ||
    signed.toString() === failed.toString() ||
    element.getAttribute("src") !== failedSrc ||
    !element.isConnected
  ) {
    giveUp(element);
    return;
  }

  const next = failedSrc.startsWith("/")
    ? `${signed.pathname}${signed.search}`
    : signed.toString();
  repaired.set(element, next);
  if (element instanceof HTMLSourceElement) {
    element.src = next;
    const parent = element.parentElement;
    if (parent instanceof HTMLMediaElement) parent.load();
    return;
  }
  const resumeAt =
    element instanceof HTMLMediaElement ? element.currentTime : 0;
  element.src = next;
  if (element instanceof HTMLMediaElement && resumeAt > 0) {
    element.addEventListener(
      "loadedmetadata",
      () => {
        element.currentTime = resumeAt;
      },
      { once: true },
    );
  }
}

function onError(event: Event): void {
  if (passthrough.has(event)) return;
  const element = event.target;
  if (!isMediaElement(element)) return;
  const src = element.getAttribute("src");
  if (!src || repaired.get(element) === src) return;
  const url = repairableMediaUrl(src);
  if (!url) return;

  // Hold the error back from the element's own handlers while we retry.
  event.stopImmediatePropagation();
  repaired.set(element, src);
  void repair(element, url, src).catch(() => giveUp(element));
}

/** Starts repairing private media for the page; returns the uninstaller. */
export function installMediaCapRepair(): () => void {
  if (typeof document === "undefined") return () => undefined;
  document.addEventListener("error", onError, true);
  return () => document.removeEventListener("error", onError, true);
}
