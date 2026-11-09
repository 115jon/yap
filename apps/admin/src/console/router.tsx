/**
 * The console's routes (TanStack Router, code-based: six paths do not need
 * the file-route generator the main app uses).
 *
 *   /                 → /people
 *   /people           People
 *   /people/$userId   People with that account's panel open
 *   /live             Live ops
 *   /insights         Insights (its own chunk, loaded on first visit)
 *   /switches         Switches
 *   /audit            Audit log
 *
 * The Worker serves the page at each of these paths to an admin session
 * only (worker.ts CONSOLE_ROUTES), so deep links and refresh work. Every
 * section route has its own error boundary and pending skeleton, so one
 * section failing leaves the frame and the others working.
 */

import {
  createRootRoute,
  createRoute,
  createRouter,
  lazyRouteComponent,
  Outlet,
  redirect,
  useNavigate,
  useRouter,
  useRouterState,
  type ErrorComponentProps,
  type RouterHistory,
} from "@tanstack/react-router";
import { createContext, useCallback, useContext } from "react";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";

import {
  AdminConsole,
  isAdminSection,
  type AdminSection,
} from "./AdminConsole";
import { AuditSection } from "./AuditSection";
import { LiveSection } from "./LiveSection";
import { PeopleSection } from "./PeopleSection";
import { SwitchesSection } from "./SwitchesSection";

/** What the frame needs from whoever mounts the router. */
export interface ConsoleSession {
  adminId: string;
  onSignOut?: () => void;
  /** Fixtures: open the palette on mount. */
  initialPaletteOpen?: boolean;
}

export const ConsoleSessionContext = createContext<ConsoleSession | null>(null);

/** The section and open account a pathname names. */
export function parseConsolePath(pathname: string): {
  section: AdminSection;
  user: string | null;
} {
  const [, first = "", second] = pathname.split("/");
  const section = isAdminSection(first) ? first : "people";
  const user =
    section === "people" && second ? decodeURIComponent(second) : null;
  return { section, user };
}

/** The path for a section (and, in People, an open account). */
export function consolePath(section: AdminSection, user: string | null) {
  if (section === "people" && user) {
    return `/people/${encodeURIComponent(user)}`;
  }
  return `/${section}`;
}

function ConsoleFrame() {
  const session = useContext(ConsoleSessionContext);
  if (!session) throw new Error("ConsoleFrame outside ConsoleSessionContext");
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const navigate = useNavigate();
  const router = useRouter();
  const { section, user } = parseConsolePath(pathname);

  const onNavigate = useCallback(
    (next: AdminSection, nextUser: string | null) => {
      const to = consolePath(next, nextUser);
      // The router's own location: fixtures and tests run on memory history.
      const here = router.state.location.pathname;
      if (to === here) return;
      // Moving within a section replaces; a new section is a history step.
      void navigate({
        to,
        replace: parseConsolePath(here).section === next,
      });
    },
    [navigate, router],
  );

  return (
    <AdminConsole
      adminId={session.adminId}
      section={section}
      selectedUserId={user}
      onNavigate={onNavigate}
      onSignOut={session.onSignOut}
      initialPaletteOpen={session.initialPaletteOpen}
    >
      <Outlet />
    </AdminConsole>
  );
}

/** A section's loading state while its code or first data arrives. */
export function SectionSkeleton() {
  return (
    <div
      data-section-pending=""
      aria-busy="true"
      className="flex min-w-0 flex-1 flex-col gap-3 p-4"
    >
      <Skeleton className="h-8 w-64" />
      <Skeleton className="h-40 w-full" />
      <Skeleton className="h-40 w-full" />
    </div>
  );
}

/** A section that threw while rendering. The frame and nav keep working. */
export function SectionError({ reset }: ErrorComponentProps) {
  const router = useRouter();
  return (
    <div
      role="alert"
      className="flex min-w-0 flex-1 items-center justify-center p-6"
    >
      <EmptyState
        title="This section hit a problem"
        description="The rest of the console still works. Try the section again, or pick another one."
        action={
          <Button
            variant="secondary"
            size="sm"
            onClick={() => {
              reset();
              void router.invalidate();
            }}
          >
            Try again
          </Button>
        }
      />
    </div>
  );
}

function SectionNotFound() {
  const navigate = useNavigate();
  return (
    <div className="flex min-w-0 flex-1 items-center justify-center p-6">
      <EmptyState
        title="Nothing here"
        description="That address isn't a console section."
        action={
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void navigate({ to: "/people", replace: true })}
          >
            Open People
          </Button>
        }
      />
    </div>
  );
}

const rootRoute = createRootRoute({
  component: ConsoleFrame,
  notFoundComponent: SectionNotFound,
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/people", replace: true });
  },
});

const peopleRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "people",
  // The table stays mounted while the account panel opens and closes.
  component: PeopleSection,
});
const peopleIndexRoute = createRoute({
  getParentRoute: () => peopleRoute,
  path: "/",
});
const personRoute = createRoute({
  getParentRoute: () => peopleRoute,
  path: "$userId",
});

const liveRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "live",
  component: LiveSection,
});
const insightsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "insights",
  component: lazyRouteComponent(() => import("./InsightsSection")),
});
const switchesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "switches",
  component: SwitchesSection,
});
const auditRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "audit",
  component: AuditSection,
});

export const consoleRouteTree = rootRoute.addChildren([
  indexRoute,
  peopleRoute.addChildren([peopleIndexRoute, personRoute]),
  liveRoute,
  insightsRoute,
  switchesRoute,
  auditRoute,
]);

export function createConsoleRouter(history?: RouterHistory) {
  return createRouter({
    routeTree: consoleRouteTree,
    history,
    defaultErrorComponent: SectionError,
    defaultPendingComponent: SectionSkeleton,
    // Sections fetch their own data; a pending route shows at once.
    defaultPendingMs: 0,
    scrollRestoration: false,
  });
}

export type ConsoleRouter = ReturnType<typeof createConsoleRouter>;
