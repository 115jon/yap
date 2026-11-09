/**
 * The admin Worker's page and its public bundle: the auth machine and Kova
 * sign-in, nothing else. No app wording and no console code: the console is
 * a separate bundle (src/console/app.tsx) that the Worker serves to an
 * admin session only, loaded here once the session cookie is set, without
 * a reload. Kova's hosted sign-ins return to "/" with their code, which the
 * SDK exchanges.
 *
 * `vite --mode fixtures` (apps/admin README) mounts the console on mock data
 * instead (../console/fixtures.tsx); that branch is dropped from builds.
 */

import { KovaAuthProvider } from "@kova/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { Boot } from "./Boot";

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

const root = createRoot(document.getElementById("root")!);

if (import.meta.env.MODE === "fixtures") {
  void import("../console/fixtures-entry").then(({ renderFixtures }) =>
    renderFixtures(root),
  );
} else {
  root.render(
    <StrictMode>
      {PUBLISHABLE_KEY ? (
        <KovaAuthProvider
          publishableKey={PUBLISHABLE_KEY}
          authUrl={AUTH_URL}
          afterSignInUrl="/"
          afterSignOutUrl="/"
        >
          <Boot />
        </KovaAuthProvider>
      ) : (
        <main className="boot-page">
          <div className="boot-card">
            <h1>Sign in</h1>
            <p className="boot-note">Not available.</p>
          </div>
        </main>
      )}
    </StrictMode>,
  );
}
