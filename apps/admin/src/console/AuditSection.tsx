/**
 * The admin audit log: every mutating admin action, newest first, with who,
 * what, on whom, the before and after values, and the request's cf-ray.
 */

import { useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Num } from "@/components/ui/num";
import { copyToClipboard } from "@/components/ui/toast";
import type { AdminAuditEntry, AdminAuditResponse } from "../shared/admin-api";
import { describeApiError } from "@/lib/api-errors";
import { getBadge } from "@/lib/badges";
import { TIER_NAMES, isTier } from "@/lib/entitlements";
import { SITE_FLAG_INFO, SITE_FLAGS, type SiteFlag } from "@/lib/site-flags";
import { cn } from "@/lib/utils";

import { RefreshButton, SectionToolbar, useAdminShell } from "./AdminConsole";
import { useAdminResource, useAdminTransport } from "./admin-data";
import { formatAgo, formatDate, formatDateTime } from "./admin-format";
import {
  Freshness,
  LoadError,
  SearchField,
  SkeletonRows,
  useAdminNow,
} from "./admin-ui";

const ACTION_LABEL: Record<string, string> = {
  "plan.set": "Plan",
  "badge.award": "Badge given",
  "badge.revoke": "Badge taken",
  "user.suspend": "Suspended",
  "user.unsuspend": "Unsuspended",
  "user.disconnect": "Disconnected",
  "flags.update": "Switch",
  "announcement.update": "Banner",
  "native_handoff.update": "Native handoff",
  "catalog.refresh": "Catalog",
};

function obj(value: unknown): Record<string, unknown> {
  return value && typeof value === "object"
    ? (value as Record<string, unknown>)
    : {};
}

function tierName(value: unknown): string {
  return isTier(value) ? TIER_NAMES[value] : "Free";
}

/** One line saying what changed, in the console's words. */
export function describeChange(entry: AdminAuditEntry): string {
  const before = obj(entry.before);
  const after = obj(entry.after);
  switch (entry.action) {
    case "plan.set": {
      const end =
        typeof after.expires_at === "string"
          ? ` until ${formatDate(after.expires_at)}`
          : "";
      return `${tierName(before.tier)} → ${tierName(after.tier)}${end}`;
    }
    case "badge.award":
    case "badge.revoke": {
      const id = String(after.badge_id ?? before.badge_id ?? "");
      return getBadge(id)?.name ?? id;
    }
    case "user.suspend":
      return typeof after.suspended_reason === "string" &&
        after.suspended_reason
        ? `“${after.suspended_reason}”`
        : "No reason given";
    case "user.unsuspend":
      return "Sign-in restored";
    case "user.disconnect":
      return `${Number(after.closed_sockets ?? 0)} connections closed`;
    case "flags.update":
      return Object.entries(after)
        .map(([flag, on]) =>
          (SITE_FLAGS as readonly string[]).includes(flag)
            ? `${SITE_FLAG_INFO[flag as SiteFlag].label} ${on ? "on" : "off"}`
            : `${flag} ${on ? "on" : "off"}`,
        )
        .join(", ");
    case "announcement.update":
      return entry.after && typeof after.text === "string"
        ? `“${after.text}”`
        : "Cleared";
    case "native_handoff.update":
      return `${after.enabled === true ? "On" : "Off"}, ${Number(after.rolloutPercent ?? 0)}% of installs`;
    case "catalog.refresh": {
      if (typeof after.error === "string")
        return `Refresh failed: ${after.error}`;
      if (after.requested === true) return "Refresh requested";
      const items =
        typeof after.items === "number" ? `, ${after.items} items` : "";
      return after.outcome === "not_modified"
        ? `Refreshed, unchanged${items}`
        : `Refreshed${items}`;
    }
    default:
      return entry.action;
  }
}

function targetLabel(entry: AdminAuditEntry): string {
  if (entry.target_type === "site") return "Site";
  return entry.target_name ?? entry.target_id ?? "Unknown";
}

export function AuditList({
  entries,
  compact = false,
  now,
  onOpenUser,
}: {
  entries: AdminAuditEntry[];
  compact?: boolean;
  now: number;
  onOpenUser?: (userId: string) => void;
}) {
  if (compact) {
    return (
      <ol className="flex flex-col">
        {entries.map((entry) => (
          <li
            key={entry.id}
            className="flex flex-col gap-0.5 border-b border-line py-2 last:border-b-0"
          >
            <span className="flex items-baseline gap-2">
              <span className="silkscreen text-ink-2">
                {ACTION_LABEL[entry.action] ?? entry.action}
              </span>
              <Num
                className="ml-auto text-ink-3"
                title={formatDateTime(entry.created_at)}
              >
                {formatAgo(entry.created_at, now)}
              </Num>
            </span>
            <span className="text-ui-sm text-ink-1">
              {describeChange(entry)}
            </span>
            <span className="text-ui-xs text-ink-3">
              {entry.target_type === "user" ? `${targetLabel(entry)} · ` : ""}by{" "}
              {entry.actor_name ?? entry.actor_id}
            </span>
          </li>
        ))}
      </ol>
    );
  }
  return (
    <table className="w-full min-w-[880px] border-separate border-spacing-0 text-left">
      <caption className="sr-only">Admin actions, newest first</caption>
      <thead className="sticky top-0 z-10 bg-ply-0">
        <tr className="[&>th]:silkscreen [&>th]:h-9 [&>th]:border-b [&>th]:border-line [&>th]:px-3 [&>th]:font-normal [&>th]:text-ink-3">
          <th scope="col" className="pl-4!">
            When
          </th>
          <th scope="col">Admin</th>
          <th scope="col">Action</th>
          <th scope="col">On</th>
          <th scope="col">Change</th>
          <th scope="col" className="pr-4!">
            cf-ray
          </th>
        </tr>
      </thead>
      <tbody>
        {entries.map((entry) => (
          <tr
            key={entry.id}
            className="h-11 text-ui-row [&>td]:border-b [&>td]:border-line [&>td]:px-3"
          >
            <td className="pl-4! whitespace-nowrap">
              <Num
                className="text-ink-2"
                title={formatDateTime(entry.created_at)}
              >
                {formatAgo(entry.created_at, now)}
              </Num>
            </td>
            <td className="max-w-[160px] truncate text-ink-2">
              {entry.actor_name ?? entry.actor_id}
            </td>
            <td>
              <span
                className={cn(
                  "silkscreen whitespace-nowrap",
                  entry.action === "user.suspend" ||
                    entry.action === "badge.revoke"
                    ? "text-danger-ink"
                    : "text-ink-1",
                )}
              >
                {ACTION_LABEL[entry.action] ?? entry.action}
              </span>
            </td>
            <td className="max-w-[200px]">
              {entry.target_type === "user" && entry.target_id && onOpenUser ? (
                <button
                  type="button"
                  onClick={() => onOpenUser(entry.target_id!)}
                  className="max-w-full truncate rounded-[2px] text-left font-medium text-ink-1 underline-offset-[3px] pointer-fine:hover:underline"
                >
                  {targetLabel(entry)}
                </button>
              ) : (
                <span className="truncate text-ink-2">
                  {targetLabel(entry)}
                </span>
              )}
            </td>
            <td
              className="max-w-[420px] truncate text-ink-1"
              title={describeChange(entry)}
            >
              {describeChange(entry)}
            </td>
            <td className="pr-4!">
              {entry.cf_ray ? (
                <button
                  type="button"
                  onClick={() =>
                    void copyToClipboard(entry.cf_ray!, "Copied the cf-ray")
                  }
                  className="rounded-[2px] font-mono text-[12px] text-ink-3 pointer-fine:hover:text-ink-1"
                  aria-label={`Copy cf-ray ${entry.cf_ray}`}
                >
                  {entry.cf_ray}
                </button>
              ) : (
                <span className="text-ink-3">—</span>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function AuditSection() {
  const { registerSearch, openPerson } = useAdminShell();
  const transport = useAdminTransport();
  const now = useAdminNow();
  const audit = useAdminResource<AdminAuditResponse>("/api/audit?limit=100", {
    refreshMs: 30_000,
  });
  const [older, setOlder] = useState<AdminAuditEntry[]>([]);
  const [cursor, setCursor] = useState<string | null | undefined>(undefined);
  const [loadingMore, setLoadingMore] = useState(false);
  const [moreError, setMoreError] = useState<unknown>(null);
  const [query, setQuery] = useState("");

  const entries = useMemo(() => {
    const seen = new Set<string>();
    return [...(audit.data?.entries ?? []), ...older].filter((entry) => {
      if (seen.has(entry.id)) return false;
      seen.add(entry.id);
      return true;
    });
  }, [audit.data, older]);

  const nextBefore =
    cursor === undefined ? (audit.data?.next_before ?? null) : cursor;
  const q = query.trim().toLowerCase();
  const shown = q
    ? entries.filter((entry) =>
        [
          entry.actor_name,
          entry.target_name,
          entry.target_id,
          entry.action,
          ACTION_LABEL[entry.action],
          describeChange(entry),
          entry.cf_ray,
        ]
          .filter(Boolean)
          .some((value) => String(value).toLowerCase().includes(q)),
      )
    : entries;

  const loadMore = async () => {
    if (!nextBefore) return;
    setLoadingMore(true);
    setMoreError(null);
    try {
      const page = await transport.get<AdminAuditResponse>(
        `/api/audit?limit=100&before=${encodeURIComponent(nextBefore)}`,
      );
      setOlder((current) => [...current, ...page.entries]);
      setCursor(page.next_before);
    } catch (error) {
      setMoreError(error);
    } finally {
      setLoadingMore(false);
    }
  };

  return (
    <section
      aria-label="Audit log"
      className="flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <SectionToolbar>
        <SearchField
          label="Search the audit log"
          placeholder="Search by admin, account, action or cf-ray"
          value={query}
          onChange={setQuery}
          inputRef={registerSearch}
          className="w-full max-w-[360px] max-md:max-w-none"
        />
        <div className="ml-auto flex items-center gap-3 max-md:hidden">
          <Freshness
            updatedAt={audit.updatedAt}
            error={audit.error}
            staleAfterMs={60_000}
            now={now}
          />
          <RefreshButton onRefresh={() => void audit.refresh()} />
        </div>
      </SectionToolbar>
      <div className="min-h-0 flex-1 overflow-auto overscroll-contain custom-scrollbar">
        {audit.loading && !audit.data ? (
          <SkeletonRows rows={8} />
        ) : audit.error && !audit.data ? (
          <LoadError
            error={audit.error}
            what="the audit log"
            onRetry={() => void audit.refresh()}
          />
        ) : shown.length === 0 ? (
          <EmptyState
            title={
              q ? `Nothing matches “${query.trim()}”` : "Nothing logged yet"
            }
            description={
              q
                ? "Search reads names, actions, changes and cf-ray ids."
                : "Plan changes, badges, suspensions and switch flips land here as they happen."
            }
          />
        ) : (
          <>
            <div className="max-md:hidden">
              <AuditList
                entries={shown}
                now={now}
                onOpenUser={(id) => openPerson(id)}
              />
            </div>
            <div className="px-3 md:hidden">
              <AuditList entries={shown} compact now={now} />
            </div>
            {nextBefore && !q ? (
              <div className="flex flex-col items-center gap-2 py-4">
                <Button
                  variant="secondary"
                  size="sm"
                  loading={loadingMore}
                  onClick={() => void loadMore()}
                >
                  Load older
                </Button>
                {moreError ? (
                  <p className="text-ui-xs text-danger-ink">
                    {describeApiError(moreError, "load older entries")}
                  </p>
                ) : null}
              </div>
            ) : null}
          </>
        )}
      </div>
    </section>
  );
}
