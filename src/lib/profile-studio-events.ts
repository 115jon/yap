/**
 * Opens the profile Studio (the full board editor) from anywhere: edit
 * mode's "Open Studio", Settings › Profile. ProfileStudioHost listens.
 */

export const OPEN_PROFILE_STUDIO_EVENT = "open-profile-studio";

export function dispatchOpenProfileStudioEvent() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(OPEN_PROFILE_STUDIO_EVENT));
}

/** Calls `listener` whenever the Studio is asked for; returns the cleanup. */
export function onOpenProfileStudio(listener: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  window.addEventListener(OPEN_PROFILE_STUDIO_EVENT, listener);
  return () => window.removeEventListener(OPEN_PROFILE_STUDIO_EVENT, listener);
}
