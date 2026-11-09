/**
 * The site-admin console, the admin Worker's one page. The Worker serves it
 * only to a verified site-admin session; every number on it comes from the
 * Worker's /api/*.
 *
 *   desktop  the Studio frame as a page: a 264px ply-1 nav with numbered
 *            silkscreen groups, a 48px column header, one scroller
 *   phone    a 48px header, rule tabs that scroll sideways, the section
 *
 * Keyboard first: ⌘K / Ctrl+K opens the palette, "/" focuses the section's
 * search, g then p/l/i/s/a (or 1-5) switches section, j/k move through
 * table rows, Enter opens one and Escape closes it. Nothing here animates on
 * a keyboard action.
 */

import {
  createContext,
  lazy,
  Suspense,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import {
  WorkbenchNavBranch,
  WorkbenchNavButton,
} from "@/components/chat/settings/SettingsWorkbench";
import { HomeIcon } from "@/components/chat/HomeIcon";
import { Icon, type IconName } from "@/components/ui/icon";
import { ThemeBanner } from "@/components/ui/theme-slots";
import { DEFAULT_PUBLIC_WEB_URL } from "@/lib/site-defaults";
import { ThemePicker } from "./ThemePicker";
import { IconButton } from "@/components/ui/IconButton";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { Tabs } from "@/components/ui/tabs";
import { useIsMobile } from "@/hooks/useMediaQuery";
import type { AdminPeopleResponse } from "../shared/admin-api";
import { cn } from "@/lib/utils";

import { AdminPalette } from "./AdminPalette";
import { useAdminResource } from "./admin-data";
import { AuditSection } from "./AuditSection";
import { LiveSection } from "./LiveSection";
import { PeopleSection } from "./PeopleSection";
import { SwitchesSection } from "./SwitchesSection";

const InsightsSection = lazy(() => import("./InsightsSection"));

export const ADMIN_SECTIONS = [
  "people",
  "live",
  "insights",
  "switches",
  "audit",
] as const;
export type AdminSection = (typeof ADMIN_SECTIONS)[number];

export function isAdminSection(value: unknown): value is AdminSection {
  return (
    typeof value === "string" &&
    (ADMIN_SECTIONS as readonly string[]).includes(value)
  );
}

export const SECTION_INFO: Record<
  AdminSection,
  { label: string; icon: IconName; key: string; kicker: string }
> = {
  people: { label: "People", icon: "members", key: "p", kicker: "Accounts" },
  live: { label: "Live ops", icon: "signal", key: "l", kicker: "Right now" },
  insights: { label: "Insights", icon: "stats", key: "i", kicker: "Trends" },
  switches: { label: "Switches", icon: "sliders", key: "s", kicker: "Flags" },
  audit: { label: "Audit log", icon: "auditLog", key: "a", kicker: "History" },
};

// ── Shell context ───────────────────────────────────────────────────────────

interface AdminShell {
  section: AdminSection;
  selectedUserId: string | null;
  go: (section: AdminSection, userId?: string | null) => void;
  openPerson: (userId: string | null) => void;
  people: ReturnType<typeof useAdminResource<AdminPeopleResponse>>;
  /** The admin's own id (from /api/session). */
  adminId: string;
  /** Where "/" sends focus: the active section registers its search box. */
  registerSearch: (input: HTMLInputElement | null) => void;
}

const AdminShellContext = createContext<AdminShell | null>(null);

export function useAdminShell(): AdminShell {
  const shell = useContext(AdminShellContext);
  if (!shell) throw new Error("useAdminShell outside AdminConsole");
  return shell;
}

/** True when a key event belongs to a text field (no single-key shortcuts). */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag !== "INPUT") return false;
  const type = (target as HTMLInputElement).type;
  return !["checkbox", "radio", "button", "submit", "range"].includes(type);
}

function inDialog(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    !!target.closest('[role="dialog"],[role="alertdialog"]')
  );
}

// ── Console ─────────────────────────────────────────────────────────────────

export interface AdminConsoleProps {
  adminId: string;
  section: AdminSection;
  selectedUserId: string | null;
  onNavigate: (section: AdminSection, userId: string | null) => void;
  /** Fixtures: open the palette on mount. */
  initialPaletteOpen?: boolean;
  /** Ends the admin session (the nav's last row; the palette on phones). */
  onSignOut?: () => void;
}

export function AdminConsole({
  adminId,
  section,
  selectedUserId,
  onNavigate,
  initialPaletteOpen = false,
  onSignOut,
}: AdminConsoleProps) {
  const isMobile = useIsMobile();
  const people = useAdminResource<AdminPeopleResponse>("/api/users", {
    refreshMs: 60_000,
  });
  const [paletteOpen, setPaletteOpen] = useState(initialPaletteOpen);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const pendingG = useRef<number | null>(null);

  const go = useCallback(
    (next: AdminSection, userId: string | null = null) =>
      onNavigate(next, userId),
    [onNavigate],
  );
  const openPerson = useCallback(
    (userId: string | null) => onNavigate("people", userId),
    [onNavigate],
  );
  const registerSearch = useCallback((input: HTMLInputElement | null) => {
    searchRef.current = input;
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const mod = event.metaKey || event.ctrlKey;
      if (mod && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((open) => !open);
        return;
      }
      if (mod || event.altKey) return;
      if (isTypingTarget(event.target) || inDialog(event.target)) return;
      if (event.key === "/") {
        if (searchRef.current) {
          event.preventDefault();
          searchRef.current.focus();
          searchRef.current.select();
        }
        return;
      }
      const index = Number(event.key);
      if (
        Number.isInteger(index) &&
        index >= 1 &&
        index <= ADMIN_SECTIONS.length
      ) {
        go(ADMIN_SECTIONS[index - 1]!);
        return;
      }
      if (event.key === "g") {
        pendingG.current = window.setTimeout(() => {
          pendingG.current = null;
        }, 1200);
        return;
      }
      if (pendingG.current !== null) {
        window.clearTimeout(pendingG.current);
        pendingG.current = null;
        const target = ADMIN_SECTIONS.find(
          (name) => SECTION_INFO[name].key === event.key.toLowerCase(),
        );
        if (target) {
          event.preventDefault();
          go(target);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [go]);

  const shell = useMemo<AdminShell>(
    () => ({
      section,
      selectedUserId,
      go,
      openPerson,
      people,
      adminId,
      registerSearch,
    }),
    [section, selectedUserId, go, openPerson, people, adminId, registerSearch],
  );

  const info = SECTION_INFO[section];
  const content: Record<AdminSection, ReactNode> = {
    people: <PeopleSection />,
    live: <LiveSection />,
    insights: (
      <Suspense fallback={null}>
        <InsightsSection />
      </Suspense>
    ),
    switches: <SwitchesSection />,
    audit: <AuditSection />,
  };

  return (
    <AdminShellContext.Provider value={shell}>
      <div
        data-admin-console=""
        className="flex h-dvh w-full min-w-0 overflow-hidden bg-ply-0 text-ink-1"
      >
        {isMobile ? null : (
          <AdminNav section={section} onSelect={go} onSignOut={onSignOut} />
        )}
        <main
          aria-labelledby="admin-section-title"
          className="flex min-w-0 flex-1 flex-col"
        >
          <header className="flex h-12 shrink-0 items-center gap-3 border-b border-line px-4 max-md:px-3">
            {isMobile ? (
              <a
                href={`${DEFAULT_PUBLIC_WEB_URL}/chat`}
                aria-label="Open Yap"
                className="-ml-1 flex size-10 items-center justify-center rounded-[2px] text-ink-2 pointer-fine:hover:bg-tint-1"
              >
                <Icon name="back" size="md" />
              </a>
            ) : null}
            <div className="flex min-w-0 flex-1 items-baseline gap-3">
              <p className="silkscreen hidden text-ink-3 sm:block">
                {info.kicker}
              </p>
              <h1
                id="admin-section-title"
                className="truncate font-display text-[18px] leading-6 font-extrabold tracking-[-0.02em] text-ink-1"
              >
                {info.label}
              </h1>
            </div>
            <button
              type="button"
              onClick={() => setPaletteOpen(true)}
              aria-keyshortcuts="Control+K Meta+K"
              className="flex h-8 items-center gap-2 rounded-[2px] border border-line-strong px-2.5 text-ui-sm text-ink-2 pointer-fine:hover:bg-tint-1 pointer-fine:hover:text-ink-1 max-md:size-10 max-md:justify-center max-md:border-0 max-md:px-0"
            >
              <Icon name="search" size="sm" />
              <span className="max-md:sr-only">Jump to</span>
              <KbdGroup className="max-md:hidden">
                <Kbd size="sm" variant="outline">
                  Ctrl
                </Kbd>
                <Kbd size="sm" variant="outline">
                  K
                </Kbd>
              </KbdGroup>
            </button>
          </header>
          {isMobile ? (
            <div className="shrink-0 border-b border-line px-1">
              <Tabs
                label="Console sections"
                scrollable
                items={ADMIN_SECTIONS.map((name) => ({
                  value: name,
                  label: SECTION_INFO[name].label,
                }))}
                value={section}
                onValueChange={(next) => go(next)}
              />
            </div>
          ) : null}
          <div className="flex min-h-0 flex-1">{content[section]}</div>
        </main>
      </div>
      <AdminPalette
        open={paletteOpen}
        onOpenChange={setPaletteOpen}
        onSignOut={onSignOut}
      />
    </AdminShellContext.Provider>
  );
}

function AdminNav({
  section,
  onSelect,
  onSignOut,
}: {
  section: AdminSection;
  onSelect: (section: AdminSection) => void;
  onSignOut?: () => void;
}) {
  return (
    <nav
      aria-label="Console"
      className="flex w-[264px] shrink-0 flex-col border-r border-line bg-ply-1"
    >
      <div className="relative flex h-12 shrink-0 items-center gap-2.5 border-b border-line px-4">
        <HomeIcon className="size-6 shrink-0" />
        <ThemeBanner />
        <span className="font-display text-[17px] leading-6 font-extrabold tracking-[-0.02em] text-ink-1">
          Yap
        </span>
        <span className="silkscreen relative z-10 ml-auto mr-6 text-ink-3">
          Admin
        </span>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-2 pt-4">
        <WorkbenchNavBranch title="Console" index={1}>
          {ADMIN_SECTIONS.map((name, index) => (
            <WorkbenchNavButton
              key={name}
              item={{
                id: name,
                label: SECTION_INFO[name].label,
                icon: SECTION_INFO[name].icon,
                trailing: (
                  <Kbd size="sm" variant="outline" className="opacity-80">
                    {index + 1}
                  </Kbd>
                ),
              }}
              current={section === name}
              narrow={false}
              tabbable
              onSelect={() => onSelect(name)}
            />
          ))}
        </WorkbenchNavBranch>
      </div>
      <div className="flex shrink-0 flex-col gap-1 border-t border-line p-2">
        <ThemePicker />
        <a
          href={`${DEFAULT_PUBLIC_WEB_URL}/chat`}
          className="flex h-9 items-center gap-3 rounded-[2px] px-2 text-ui-row font-medium text-ink-2 pointer-fine:hover:bg-tint-1 pointer-fine:hover:text-ink-1"
        >
          <Icon name="externalLink" size="lg" className="text-ink-3" />
          Open Yap
        </a>
        {onSignOut ? (
          <button
            type="button"
            onClick={onSignOut}
            className="flex h-9 items-center gap-3 rounded-[2px] px-2 text-left text-ui-row font-medium text-ink-2 pointer-fine:hover:bg-tint-1 pointer-fine:hover:text-ink-1"
          >
            <Icon name="logout" size="lg" className="text-ink-3" />
            Sign out
          </button>
        ) : null}
        <p className="flex items-center gap-2 px-2 pb-1 text-ui-xs text-ink-3">
          <KbdGroup>
            <Kbd size="sm" variant="outline">
              g
            </Kbd>
            <Kbd size="sm" variant="outline">
              p
            </Kbd>
          </KbdGroup>
          jumps between sections
        </p>
      </div>
    </nav>
  );
}

/** The section's toolbar row: search, filters, and the freshness readout. */
export function SectionToolbar({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex min-h-14 shrink-0 flex-wrap items-center gap-x-4 gap-y-2 border-b border-line px-4 py-2 max-md:px-3",
        className,
      )}
    >
      {children}
    </div>
  );
}

/** A reload key with an accessible name; spins while a fetch runs. */
export function RefreshButton({
  onRefresh,
  busy,
}: {
  onRefresh: () => void;
  busy?: boolean;
}) {
  return (
    <IconButton
      icon="refresh"
      tooltip="Refresh"
      size={32}
      onClick={onRefresh}
      aria-busy={busy || undefined}
      iconClassName={busy ? "motion-safe:animate-spin" : undefined}
    />
  );
}
