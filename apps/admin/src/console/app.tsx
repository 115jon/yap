/**
 * The console bundle's entry, served by the Worker to an admin session only
 * and imported by the boot page (../boot/Boot.tsx) once the session is set.
 * It brings its own stylesheet, a TanStack Query client, the API transport
 * and the router (./router.tsx).
 *
 * The boot page owns the session: `active` false holds every query while it
 * gets a new one, `epoch` bumps when it has, and `onSessionLost` is how a
 * request tells it the session is gone.
 */

import "./console.css";

import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { useEffect, useRef, useState, type ReactNode } from "react";

import type { ConsoleAppProps } from "../shared/console-contract";
import {
  AdminActiveContext,
  AdminTransportContext,
  createAdminQueryClient,
  createHttpTransport,
  invalidateAfterWrite,
  type AdminTransport,
} from "./admin-data";
import {
  ConsoleSessionContext,
  createConsoleRouter,
  type ConsoleRouter,
  type ConsoleSession,
} from "./router";
import { ConsoleShell } from "./shell";
import type { QueryClient } from "@tanstack/react-query";

const BODY_CLASSES = ["bg-ply-0", "text-ink-1", "antialiased", "font-ui"];

/** Everything a router of console routes needs above it. */
export function ConsoleProviders({
  client,
  transport,
  active = true,
  session,
  children,
}: {
  client: QueryClient;
  transport: AdminTransport;
  active?: boolean;
  session: ConsoleSession;
  children: ReactNode;
}) {
  return (
    <QueryClientProvider client={client}>
      <AdminTransportContext.Provider value={transport}>
        <AdminActiveContext.Provider value={active}>
          <ConsoleSessionContext.Provider value={session}>
            {children}
          </ConsoleSessionContext.Provider>
        </AdminActiveContext.Provider>
      </AdminTransportContext.Provider>
    </QueryClientProvider>
  );
}

export default function ConsoleApp({
  adminId,
  active,
  epoch,
  onSessionLost,
  onSignOut,
}: ConsoleAppProps) {
  const [client] = useState(createAdminQueryClient);
  // Made once: the boot page's onSessionLost is stable for the page's life.
  const [transport] = useState(() =>
    createHttpTransport({
      onSessionLost,
      onWrite: (path) => invalidateAfterWrite(client, path),
    }),
  );
  const [router] = useState<ConsoleRouter>(() => createConsoleRouter());

  // A new session after a lost one: refetch what is shown.
  const firstEpoch = useRef(epoch);
  useEffect(() => {
    if (epoch === firstEpoch.current) return;
    void client.invalidateQueries();
  }, [epoch, client]);

  useEffect(() => {
    document.body.classList.add(...BODY_CLASSES);
    document.title = "Yap admin";
    return () => {
      document.body.classList.remove(...BODY_CLASSES);
      document.title = "Sign in";
    };
  }, []);

  return (
    <ConsoleShell>
      <ConsoleProviders
        client={client}
        transport={transport}
        active={active}
        session={{ adminId, onSignOut }}
      >
        <RouterProvider router={router} />
      </ConsoleProviders>
    </ConsoleShell>
  );
}
