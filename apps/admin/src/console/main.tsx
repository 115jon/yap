/**
 * The console page's entry. The Worker serves this page only to a verified
 * site-admin session; it asks GET /api/session for the admin's id and
 * mounts the console. Sections live in the query (?section=live&user=…).
 *
 * `vite --mode fixtures` (apps/admin README) mounts the same console on mock
 * data instead (./fixtures.tsx); that branch is dropped from builds.
 */

import "./console.css";

import { KovaAuthProvider, useAuth } from "@kova/react";
import { ThemeProvider } from "next-themes";
import { StrictMode, useCallback, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { DEFAULT_KOVA_AUTH_URL } from "@/lib/site-defaults";
import { NEXT_THEME_NAMES } from "@/lib/theme-preferences";

import {
  AdminConsole,
  isAdminSection,
  type AdminSection,
} from "./AdminConsole";
import { liveTransport } from "./admin-data";

function meta(name: string): string {
  const value =
    document.querySelector(`meta[name="${name}"]`)?.getAttribute("content") ??
    "";
  return value.startsWith("__") ? "" : value;
}

type Location = { section: AdminSection; user: string | null };

function readLocation(): Location {
  const params = new URLSearchParams(window.location.search);
  const section = params.get("section");
  return {
    section: isAdminSection(section) ? section : "people",
    user: params.get("user") || null,
  };
}

function useLocationState(): [Location, (next: Location) => void] {
  const [location, setLocation] = useState(readLocation);
  useEffect(() => {
    const onPop = () => setLocation(readLocation());
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);
  const navigate = useCallback((next: Location) => {
    const params = new URLSearchParams();
    if (next.section !== "people") params.set("section", next.section);
    if (next.user) params.set("user", next.user);
    const query = params.toString();
    const url = `/${query ? `?${query}` : ""}`;
    const current = readLocation();
    // Moving within a section replaces; a new section is a history step.
    if (current.section === next.section)
      window.history.replaceState(null, "", url);
    else window.history.pushState(null, "", url);
    setLocation(next);
  }, []);
  return [location, navigate];
}

function Console() {
  const [adminId, setAdminId] = useState<string | null>(null);
  const [location, navigate] = useLocationState();
  const { signOut } = useAuth();

  useEffect(() => {
    liveTransport
      .get<{ user_id: string }>("/api/session")
      .then((session) => setAdminId(session.user_id))
      .catch(() => window.location.replace("/"));
  }, []);

  const endSession = useCallback(async () => {
    await fetch("/api/sign-out", {
      method: "POST",
      credentials: "same-origin",
    }).catch(() => null);
    await signOut().catch(() => {});
    window.location.replace("/");
  }, [signOut]);

  if (!adminId) return <div className="h-dvh bg-ply-0" />;
  return (
    <AdminConsole
      adminId={adminId}
      section={location.section}
      selectedUserId={location.user}
      onNavigate={(section, user) => navigate({ section, user })}
      onSignOut={() => void endSession()}
    />
  );
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      disableTransitionOnChange
      themes={NEXT_THEME_NAMES}
    >
      <TooltipProvider>
        {children}
        <Toaster />
      </TooltipProvider>
    </ThemeProvider>
  );
}

const root = createRoot(document.getElementById("root")!);

if (import.meta.env.MODE === "fixtures") {
  void import("./fixtures").then(({ FixturesPage }) =>
    root.render(
      <StrictMode>
        <Shell>
          <FixturesPage />
        </Shell>
      </StrictMode>,
    ),
  );
} else {
  root.render(
    <StrictMode>
      <Shell>
        <KovaAuthProvider
          publishableKey={meta("kova-key")}
          authUrl={meta("kova-url") || DEFAULT_KOVA_AUTH_URL}
          afterSignOutUrl="/"
        >
          <Console />
        </KovaAuthProvider>
      </Shell>
    </StrictMode>,
  );
}
