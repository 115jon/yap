/**
 * The admin Worker's public page: Kova sign-in and nothing else. No app
 * wording, no console code (that is served only to an admin session).
 *
 * Signed in to the "Yap Admin" Kova app, it trades the app token for the
 * Worker's HttpOnly session cookie (POST /api/session) and reloads into the
 * console. Any refusal is a bare 404; the page then signs the Kova session
 * out so a non-admin is not left signed in. Kova's hosted sign-ins return to
 * "/" with their code, which this page (the SDK) exchanges.
 */

import { KovaAuthProvider, SignIn, useAuth } from "@kova/react";
import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

/** A setting the Worker wrote into the page (placeholders mean unset). */
function meta(name: string): string {
  const value =
    document.querySelector(`meta[name="${name}"]`)?.getAttribute("content") ??
    "";
  return value.startsWith("__") ? "" : value;
}

const PUBLISHABLE_KEY =
  meta("kova-key") || (import.meta.env.VITE_KOVA_ADMIN_PUBLISHABLE_KEY ?? "");
// The Worker always fills it (KOVA_AUTH_URL or Yap's default); `vite dev`
// without the Worker uses the same default.
const AUTH_URL = meta("kova-url") || "https://auth.115jon.com";

type State = "idle" | "checking" | "refused";

function Gate() {
  const { isLoaded, isSignedIn, getToken, signOut } = useAuth();
  const [state, setState] = useState<State>("idle");

  useEffect(() => {
    if (!isLoaded || !isSignedIn || state !== "idle") return;
    let cancelled = false;
    // eslint-disable-next-line react-hooks-js/set-state-in-effect
    setState("checking");
    void (async () => {
      const token = await getToken().catch(() => null);
      const response = token
        ? await fetch("/api/session", {
            method: "POST",
            credentials: "same-origin",
            headers: { Authorization: `Bearer ${token}` },
          }).catch(() => null)
        : null;
      if (cancelled) return;
      if (response?.status === 204) {
        window.location.replace("/");
        return;
      }
      setState("refused");
      await signOut().catch(() => {});
    })();
    return () => {
      cancelled = true;
    };
  }, [isLoaded, isSignedIn, state, getToken, signOut]);

  if (!isLoaded || state === "checking") {
    return <p className="note">One moment…</p>;
  }
  return (
    <>
      {state === "refused" ? (
        <p className="note" role="alert">
          That account can't sign in here.
        </p>
      ) : null}
      <SignIn afterSignInUrl={`${window.location.origin}/`} signUpUrl={null} />
    </>
  );
}

const style = document.createElement("style");
style.textContent = `
  :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
  body { margin: 0; min-height: 100dvh; display: grid; place-items: center; }
  main { width: min(400px, calc(100vw - 32px)); padding: 32px 0; }
  h1 { font-size: 20px; margin: 0 0 16px; }
  .note { font-size: 14px; margin: 0 0 16px; }
`;
document.head.appendChild(style);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <main>
      <h1>Sign in</h1>
      {PUBLISHABLE_KEY ? (
        <KovaAuthProvider
          publishableKey={PUBLISHABLE_KEY}
          authUrl={AUTH_URL}
          afterSignInUrl="/"
          afterSignOutUrl="/"
        >
          <Gate />
        </KovaAuthProvider>
      ) : (
        <p className="note">Not available.</p>
      )}
    </main>
  </StrictMode>,
);
