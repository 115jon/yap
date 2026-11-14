import { useEffect, useSyncExternalStore } from "react";

/**
 * Whether the app is worth drawing (talk-tree-spec §5f): inactive while the
 * document is hidden (background tab, minimised window). An unfocused tab
 * stays active, so a stream on a second monitor keeps playing.
 *
 * One module-level store feeds every caller, with the listener attached on
 * the first subscriber and removed after the last. SSR and tests see
 * "active".
 *
 *   const active = useAppActive();
 *   useEffect(() => { if (!active) return; const id = requestAnimationFrame(…); … }, [active]);
 *
 * `<AppActiveRoot />` (mounted once in routes/__root.tsx) mirrors the state
 * onto `<html data-app-inactive>`, where one CSS rule pauses every
 * animation.
 */

type Activity = {
  visible: boolean;
};

let active = true;
const listeners = new Set<() => void>();
let detach: (() => void) | null = null;

export function computeAppActive(state: Activity): boolean {
  return state.visible;
}

function update(state: Activity) {
  const next = computeAppActive(state);
  if (next === active) return;
  active = next;
  listeners.forEach((listener) => listener());
}

function attach(): () => void {
  if (typeof document === "undefined") return () => {};
  const onVisibility = () =>
    update({ visible: document.visibilityState !== "hidden" });
  try {
    document.addEventListener("visibilitychange", onVisibility);
    onVisibility();
  } catch {
    // The app just stays "active", as it does under SSR.
    return () => {};
  }
  return () => document.removeEventListener("visibilitychange", onVisibility);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  if (listeners.size === 1) detach = attach();
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      detach?.();
      detach = null;
    }
  };
}

const getSnapshot = () => active;
const getServerSnapshot = () => true;

export function useAppActive(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}

/** Mount once at the root: mirrors activity onto `<html data-app-inactive>`. */
export function AppActiveRoot() {
  const isActive = useAppActive();
  useEffect(() => {
    const root = document.documentElement;
    if (isActive) root.removeAttribute("data-app-inactive");
    else root.setAttribute("data-app-inactive", "");
  }, [isActive]);
  useEffect(() => {
    return () => {
      document.documentElement.removeAttribute("data-app-inactive");
    };
  }, []);
  return null;
}
