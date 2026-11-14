import {
  clearDesktopAuthSession,
  markAuthLogoutIntent,
} from "@/lib/desktop-auth";
import { endKovaBrowserSession } from "@/lib/kova-sign-out";

/**
 * Sign out on this device and land on /sign-in: settings' Sign out, and the
 * last step of asking for account deletion.
 */
export function signOutToSignIn(
  signOut: (callbackURL?: string) => Promise<void>,
): void {
  markAuthLogoutIntent();
  clearDesktopAuthSession();
  // Auth-domain session first, so the SDK can't mint a new token from it
  // mid sign-out and land /sign-in signed in again.
  void endKovaBrowserSession()
    .then(() => signOut("/sign-in"))
    .finally(() => {
      if (typeof window !== "undefined") {
        window.location.replace("/sign-in");
      }
    });
}
