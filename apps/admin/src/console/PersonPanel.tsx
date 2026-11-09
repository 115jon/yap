/**
 * One account, beside the table: identity, readouts, plan, badges, sign-in
 * (suspension, devices, live sockets), servers and the account's audit
 * history.
 *
 * Mutations are optimistic but confirmed: the change shows at once, the
 * server's answer replaces it, and a refusal rolls it back with the server's
 * reason. A plan change and a suspension ask first.
 */

import { useState, type ReactNode } from "react";

import { Avatar } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { EmptyState } from "@/components/ui/empty-state";
import { IconButton } from "@/components/ui/IconButton";
import { FieldPlate } from "@/components/ui/input";
import { LedLabel } from "@/components/ui/led";
import { Num } from "@/components/ui/num";
import { RuleCard } from "@/components/ui/plate";
import { SegmentedControl } from "@/components/ui/segmented-control";
import { SidePanel } from "@/components/ui/side-panel";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { copyToClipboard, toast } from "@/components/ui/toast";
import { BadgeArt } from "@/components/ui/user-badges";
import type {
  AdminPerson,
  AdminPersonDetail,
  AdminPeopleResponse,
} from "../shared/admin-api";
import { describeApiError } from "@/lib/api-errors";
import { BADGES, type BadgeDefinition } from "@/lib/badges";
import {
  effectiveTier,
  TIER_NAMES,
  type Entitlement,
  type Tier,
} from "@/lib/entitlements";
import { assetUrl } from "./admin-format";
import { cn } from "@/lib/utils";

import { useAdminShell } from "./AdminConsole";
import { useAdminResource, useAdminTransport } from "./admin-data";
import {
  formatAgo,
  formatBytes,
  formatCount,
  formatDate,
  formatDateTime,
  formatHours,
  parseTime,
} from "./admin-format";
import { PlanChip, PresenceWord, useAdminNow } from "./admin-ui";
import { AuditList } from "./AuditSection";

const DAY_MS = 86_400_000;

type Extension = "30" | "90" | "365" | "none";

function displayName(person: AdminPerson): string {
  return person.display_name?.trim() || person.username;
}

/** Replace one person in the shared people list. */
function patchPeople(
  people: AdminPeopleResponse | null,
  person: AdminPerson,
): AdminPeopleResponse | null {
  if (!people) return people;
  return {
    ...people,
    people: people.people.map((row) => (row.id === person.id ? person : row)),
  };
}

export function PersonPanel({
  userId,
  person: listed,
  online,
  onClose,
}: {
  userId: string | null;
  person: AdminPerson | null;
  online: boolean | null;
  onClose: () => void;
}) {
  const detail = useAdminResource<AdminPersonDetail>(
    userId ? `/api/users/${encodeURIComponent(userId)}` : null,
  );
  const person = detail.data?.person ?? listed;

  return (
    <SidePanel
      open={!!userId}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title="Account"
      description="Plan, badges, sign-in and history for one account"
      dockClassName="w-[420px] max-xl:w-[380px]"
      // The panel is a route (/people/$userId): back and forward are the
      // router's, so the panel must not push its own history entry.
      backButton={false}
    >
      <div
        data-admin-panel=""
        className="min-h-0 flex-1 overflow-y-auto overscroll-contain custom-scrollbar"
      >
        {!person ? (
          detail.error ? (
            <EmptyState
              size="sm"
              title="Couldn't load this account"
              description={describeApiError(detail.error, "load this account")}
              action={
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void detail.refresh()}
                >
                  Try again
                </Button>
              }
            />
          ) : (
            <PanelSkeleton />
          )
        ) : (
          <PersonBody
            person={person}
            detail={detail.data}
            online={online}
            onPersonChange={(next) =>
              detail.mutate((current) =>
                current ? { ...current, person: next } : current,
              )
            }
            onReload={() => void detail.refresh()}
          />
        )}
      </div>
    </SidePanel>
  );
}

function PanelSkeleton() {
  return (
    <div aria-hidden="true" className="flex flex-col gap-4 p-4">
      <div className="flex items-center gap-3">
        <Skeleton className="size-16 rounded-avatar" />
        <div className="flex flex-col gap-2">
          <Skeleton className="h-4 w-36 rounded-[2px]" />
          <Skeleton className="h-3 w-24 rounded-[2px]" />
        </div>
      </div>
      <Skeleton className="h-24 rounded-[2px]" />
      <Skeleton className="h-40 rounded-[2px]" />
    </div>
  );
}

function PersonBody({
  person,
  detail,
  online,
  onPersonChange,
  onReload,
}: {
  person: AdminPerson;
  detail: AdminPersonDetail | null;
  online: boolean | null;
  onPersonChange: (person: AdminPerson) => void;
  onReload: () => void;
}) {
  const { people, adminId } = useAdminShell();
  const now = useAdminNow();
  const name = displayName(person);

  /** Show `next` everywhere now; the caller confirms or rolls back. */
  const applyLocal = (next: AdminPerson) => {
    onPersonChange(next);
    people.mutate((current) => patchPeople(current, next));
  };

  return (
    <div className="flex flex-col gap-6 px-4 pt-2 pb-8">
      <div className="flex items-center gap-3">
        <Avatar
          src={person.avatar_url ? assetUrl(person.avatar_url) : null}
          name={name}
          size={64}
          presence={person.suspended_at ? "dnd" : online ? "online" : undefined}
          ring="ply-1"
        />
        <div className="flex min-w-0 flex-col gap-1">
          <p className="truncate font-display text-[20px] leading-6 font-extrabold tracking-[-0.02em] text-ink-1">
            {name}
          </p>
          <p className="truncate text-ui-sm text-ink-2">@{person.username}</p>
          <PresenceWord
            online={online}
            suspended={!!person.suspended_at}
            lastSeen={person.last_active_at}
            now={now}
          />
        </div>
      </div>

      <div className="flex min-w-0 items-center gap-2 rounded-[2px] bg-ply-2 py-1 pr-1 pl-3">
        <span className="silkscreen shrink-0 text-ink-3">Id</span>
        <code className="min-w-0 flex-1 truncate font-mono text-[12px] text-ink-2">
          {person.id}
        </code>
        <IconButton
          icon="copy"
          size={28}
          tooltip="Copy id"
          onClick={() =>
            void copyToClipboard(person.id, "Copied the account id")
          }
        />
      </div>

      <dl className="grid grid-cols-3 gap-x-4 gap-y-4 border-t-2 border-ink-1 pt-3">
        <Stat label="Messages" value={formatCount(person.messages_sent)} />
        <Stat label="Voice" value={formatHours(person.voice_seconds)} />
        <Stat label="Stream" value={formatHours(person.stream_seconds)} />
        <Stat label="Servers" value={String(person.servers_joined)} />
        <Stat label="Storage" value={formatBytes(person.storage_bytes)} />
        <Stat label="Joined" value={formatDate(person.created_at)} small />
      </dl>

      {/* Keyed so the picker starts from the stored plan whenever it changes. */}
      <PlanCard
        key={`${person.id}:${effectiveTier(person.plan, now)}`}
        person={person}
        now={now}
        applyLocal={applyLocal}
      />
      <BadgesCard person={person} applyLocal={applyLocal} />
      <SignInCard
        person={person}
        detail={detail}
        isSelf={person.id === adminId}
        now={now}
        applyLocal={applyLocal}
        onReload={onReload}
      />

      <RuleCard
        legend="Servers"
        index={detail ? <Num>{detail.servers.length}</Num> : null}
      >
        {!detail ? (
          <Skeleton className="h-16 rounded-[2px]" />
        ) : detail.servers.length === 0 ? (
          <p className="text-ui-sm text-ink-3">Not in any server.</p>
        ) : (
          <ul className="flex flex-col gap-0.5">
            {detail.servers.map((server) => (
              <li
                key={server.id}
                className="flex h-9 items-center gap-2.5 px-1"
              >
                <Avatar
                  src={server.icon_url ? assetUrl(server.icon_url) : null}
                  name={server.name}
                  size={24}
                  ring="ply-1"
                />
                <span className="min-w-0 flex-1 truncate text-ui-row text-ink-1">
                  {server.name}
                </span>
                {server.owner ? (
                  <span className="silkscreen text-ink-2">Owner</span>
                ) : null}
                <Num className="text-ink-3">{server.members} members</Num>
              </li>
            ))}
          </ul>
        )}
      </RuleCard>

      <RuleCard legend="History">
        {!detail ? (
          <Skeleton className="h-16 rounded-[2px]" />
        ) : detail.audit.length === 0 ? (
          <p className="text-ui-sm text-ink-3">
            No admin has changed this account.
          </p>
        ) : (
          <AuditList entries={detail.audit} compact now={now} />
        )}
      </RuleCard>
    </div>
  );
}

function Stat({
  label,
  value,
  small,
}: {
  label: string;
  value: string;
  small?: boolean;
}) {
  return (
    <div className="min-w-0">
      <dt className="silkscreen truncate text-ink-3">{label}</dt>
      <dd
        className={cn(
          "mt-1 truncate text-ink-1 tabular-nums",
          small
            ? "text-num-sm"
            : "font-silk text-[20px] leading-6 font-semibold [font-stretch:90%]",
        )}
      >
        {value}
      </dd>
    </div>
  );
}

// ── Plan ────────────────────────────────────────────────────────────────────

function planAfter(
  current: Entitlement,
  tier: Tier,
  extension: Extension,
  now: number,
): Entitlement {
  if (tier === "free")
    return { tier: "free", source: null, started_at: null, expires_at: null };
  const startedAt =
    current.tier === tier && current.started_at
      ? current.started_at
      : new Date(now).toISOString();
  if (tier === "founder") {
    return { tier, source: "admin", started_at: startedAt, expires_at: null };
  }
  if (extension === "none") {
    return { tier, source: "admin", started_at: startedAt, expires_at: null };
  }
  // Extending Loud adds to whatever is left, not to today.
  const currentEnd =
    current.tier === "loud" && effectiveTier(current, now) === "loud"
      ? (parseTime(current.expires_at) ?? now)
      : now;
  const from = Math.max(now, currentEnd);
  return {
    tier,
    source: "admin",
    started_at: startedAt,
    expires_at: new Date(from + Number(extension) * DAY_MS).toISOString(),
  };
}

function PlanCard({
  person,
  now,
  applyLocal,
}: {
  person: AdminPerson;
  now: number;
  applyLocal: (person: AdminPerson) => void;
}) {
  const transport = useAdminTransport();
  const current = person.plan;
  const currentTier = effectiveTier(current, now);
  const [tier, setTier] = useState<Tier>(currentTier);
  const [extension, setExtension] = useState<Extension>("30");
  const [confirming, setConfirming] = useState(false);

  const next = planAfter(current, tier, extension, now);
  const unchanged =
    tier === currentTier &&
    (tier !== "loud" ||
      (current.expires_at ?? null) === (next.expires_at ?? null));
  const losesFounder = currentTier === "founder" && tier !== "founder";

  const save = async () => {
    const previous = person;
    applyLocal({ ...person, plan: next });
    try {
      const body =
        tier === "loud"
          ? { tier, expires_at: next.expires_at ?? null }
          : { tier };
      const result = await transport.send<{ person: AdminPerson | null }>(
        "POST",
        `/api/users/${encodeURIComponent(person.id)}/plan`,
        body,
      );
      if (result.person) applyLocal(result.person);
      toast.success(`${displayName(person)} is on ${TIER_NAMES[tier]}`);
    } catch (error) {
      applyLocal(previous);
      throw error;
    }
  };

  const summary =
    tier === "loud"
      ? next.expires_at
        ? `Yap Loud until ${formatDate(next.expires_at)}.`
        : "Yap Loud with no end date."
      : tier === "founder"
        ? "Founder for life, with the Founder badge."
        : "Back to the free plan.";

  return (
    <RuleCard legend="Plan" index={<PlanChip plan={current} now={now} />}>
      <div className="flex flex-col gap-3">
        <p className="text-ui-sm text-ink-2">
          {current.tier === "free" || !current.source
            ? "On the free plan."
            : `${TIER_NAMES[current.tier]} by ${current.source === "grant" ? "the founder grant" : current.source === "stripe" ? "subscription" : "an admin"}, since ${formatDate(current.started_at)}${
                current.expires_at
                  ? `, until ${formatDate(current.expires_at)}`
                  : ""
              }.`}
        </p>
        <SegmentedControl
          label="Plan"
          size="sm"
          items={[
            { value: "free", label: "Free" },
            { value: "loud", label: "Loud" },
            { value: "founder", label: "Founder" },
          ]}
          value={tier}
          onValueChange={setTier}
        />
        {tier === "loud" ? (
          <div className="flex flex-col gap-1.5">
            <p className="silkscreen text-ink-3">
              {currentTier === "loud" ? "Extend by" : "Runs for"}
            </p>
            <SegmentedControl
              label="Loud length"
              size="sm"
              items={[
                { value: "30", label: "30 d" },
                { value: "90", label: "90 d" },
                { value: "365", label: "1 yr" },
                { value: "none", label: "No end" },
              ]}
              value={extension}
              onValueChange={setExtension}
            />
          </div>
        ) : null}
        <div className="flex items-center justify-between gap-3">
          <p className="min-w-0 text-ui-xs text-ink-3">
            {unchanged ? "No change." : summary}
          </p>
          <Button
            variant="secondary"
            size="sm"
            disabled={unchanged}
            onClick={() => setConfirming(true)}
          >
            Change plan
          </Button>
        </div>
      </div>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Move ${displayName(person)} to ${TIER_NAMES[tier]}?`}
        description={
          <>
            {summary}
            {losesFounder ? " The Founder badge comes off too." : null} They see
            it the next time their profile loads. The change is written to the
            audit log.
          </>
        }
        confirmLabel="Change plan"
        errorAction="change the plan"
        onConfirm={save}
      />
    </RuleCard>
  );
}

// ── Badges ──────────────────────────────────────────────────────────────────

const KIND_LABEL: Record<BadgeDefinition["rule"]["kind"], string> = {
  manual: "By hand",
  auto: "Automatic",
  event: "Automatic",
  grant: "With Founder",
  tier: "With Loud",
};

function BadgesCard({
  person,
  applyLocal,
}: {
  person: AdminPerson;
  applyLocal: (person: AdminPerson) => void;
}) {
  const transport = useAdminTransport();
  const [busy, setBusy] = useState<string | null>(null);
  const held = new Map(person.badges.map((badge) => [badge.id, badge]));

  const toggle = async (badge: BadgeDefinition, award: boolean) => {
    const previous = person;
    const at = new Date().toISOString();
    applyLocal({
      ...person,
      badges: award
        ? [...person.badges, { id: badge.id, source: "admin", awarded_at: at }]
        : person.badges.filter((item) => item.id !== badge.id),
    });
    setBusy(badge.id);
    try {
      const result = await transport.send<{ person: AdminPerson | null }>(
        "POST",
        `/api/users/${encodeURIComponent(person.id)}/badges`,
        { badge_id: badge.id, action: award ? "award" : "revoke" },
      );
      if (result.person) applyLocal(result.person);
    } catch (error) {
      applyLocal(previous);
      toast.error(
        describeApiError(error, `${award ? "award" : "revoke"} ${badge.name}`),
      );
    } finally {
      setBusy(null);
    }
  };

  return (
    <RuleCard legend="Badges" index={<Num>{person.badges.length}</Num>}>
      <ul className="flex flex-col">
        {BADGES.map((badge) => {
          const award = held.get(badge.id);
          const manual = badge.rule.kind === "manual";
          const labelId = `admin-badge-${badge.id}`;
          return (
            <li
              key={badge.id}
              className={cn(
                "flex min-h-12 items-center gap-3 border-b border-line py-1.5 last:border-b-0",
                !award && !manual && "opacity-60",
              )}
            >
              <BadgeArt id={badge.id} size="md" />
              <div className="flex min-w-0 flex-1 flex-col">
                <span
                  id={labelId}
                  className="truncate text-ui-row font-medium text-ink-1"
                >
                  {badge.name}
                </span>
                <span className="truncate text-ui-xs text-ink-3">
                  {award
                    ? `${award.source === "admin" ? "Given by an admin" : award.source === "tier" ? "While on Loud" : "Earned"} ${award.awarded_at ? formatDate(award.awarded_at) : ""}`
                    : badge.description}
                </span>
              </div>
              {manual ? (
                <Switch
                  size="sm"
                  aria-labelledby={labelId}
                  checked={!!award}
                  disabled={busy === badge.id}
                  onCheckedChange={(checked) => void toggle(badge, checked)}
                />
              ) : (
                <span className="silkscreen shrink-0 text-ink-3">
                  {KIND_LABEL[badge.rule.kind]}
                </span>
              )}
            </li>
          );
        })}
      </ul>
    </RuleCard>
  );
}

// ── Sign-in ─────────────────────────────────────────────────────────────────

function SignInCard({
  person,
  detail,
  isSelf,
  now,
  applyLocal,
  onReload,
}: {
  person: AdminPerson;
  detail: AdminPersonDetail | null;
  isSelf: boolean;
  now: number;
  applyLocal: (person: AdminPerson) => void;
  onReload: () => void;
}) {
  const transport = useAdminTransport();
  const [confirm, setConfirm] = useState<
    "suspend" | "unsuspend" | "disconnect" | null
  >(null);
  const [reason, setReason] = useState("");
  const suspended = !!person.suspended_at;
  const live = detail?.live_sessions ?? null;

  const setSuspended = async (suspend: boolean) => {
    const previous = person;
    applyLocal({
      ...person,
      suspended_at: suspend ? new Date(now).toISOString() : null,
      suspended_reason: suspend ? reason.trim() || null : null,
    });
    try {
      const result = await transport.send<{
        person: AdminPerson | null;
        closed_sockets: number | null;
      }>("POST", `/api/users/${encodeURIComponent(person.id)}/suspension`, {
        suspended: suspend,
        reason: reason.trim() || null,
      });
      if (result.person) applyLocal(result.person);
      toast.success(
        suspend
          ? `${displayName(person)} is suspended`
          : `${displayName(person)} can sign in again`,
        suspend && result.closed_sockets
          ? {
              description: `Closed ${result.closed_sockets} live connection${result.closed_sockets === 1 ? "" : "s"}.`,
            }
          : undefined,
      );
      setReason("");
      onReload();
    } catch (error) {
      applyLocal(previous);
      throw error;
    }
  };

  const disconnect = async () => {
    const result = await transport.send<{ closed_sockets: number }>(
      "POST",
      `/api/users/${encodeURIComponent(person.id)}/disconnect`,
    );
    toast.success(
      result.closed_sockets
        ? `Closed ${result.closed_sockets} connection${result.closed_sockets === 1 ? "" : "s"}`
        : "Nothing was connected",
    );
    onReload();
  };

  return (
    <RuleCard legend="Sign-in">
      <div className="flex flex-col gap-4">
        {suspended ? (
          <div className="flex flex-col gap-1 rounded-[2px] bg-danger-tint px-3 py-2.5">
            <LedLabel state="muted">
              Suspended {formatAgo(person.suspended_at, now)}
            </LedLabel>
            <p className="text-ui-sm text-ink-1">
              {person.suspended_reason ?? "No reason given."}
            </p>
          </div>
        ) : null}

        <div className="flex flex-col gap-1.5">
          <p className="silkscreen text-ink-3">Live connections</p>
          {live === null ? (
            <p className="text-ui-sm text-ink-3">
              {detail ? "The gateway didn't answer." : "Checking…"}
            </p>
          ) : live.length === 0 ? (
            <p className="text-ui-sm text-ink-3">Not connected.</p>
          ) : (
            <ul className="flex flex-col gap-0.5">
              {live.map((session, index) => (
                <li
                  key={index}
                  className="flex h-8 items-center gap-2 text-ui-sm text-ink-2"
                >
                  <LedLabel
                    state={session.voice_channel_id ? "talk" : "off"}
                    tone="plain"
                  >
                    {session.platform ?? "web"}
                  </LedLabel>
                  <span className="truncate text-ink-3">
                    {session.voice_channel_id
                      ? "in voice"
                      : (session.status ?? "online")}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>

        <DeviceList detail={detail} now={now} />

        <p className="text-ui-xs text-pretty text-ink-3">
          Kova owns sign-in sessions, and Yap holds no key that can list or end
          them, so there is no “sign out everywhere” here. Suspending refuses
          every request from the account at once and closes its live
          connections; lifting it lets the same sessions back in.
        </p>

        <div className="flex flex-wrap gap-2">
          {suspended ? (
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setConfirm("unsuspend")}
            >
              Lift suspension
            </Button>
          ) : (
            <Button
              variant="destructive"
              size="sm"
              disabled={isSelf}
              title={isSelf ? "You can't suspend your own account" : undefined}
              onClick={() => setConfirm("suspend")}
            >
              Suspend sign-in
            </Button>
          )}
          <Button
            variant="ghost"
            size="sm"
            disabled={!live || live.length === 0}
            onClick={() => setConfirm("disconnect")}
          >
            Close live connections
          </Button>
        </div>
      </div>

      <ConfirmDialog
        open={confirm === "suspend"}
        onOpenChange={(open) => setConfirm(open ? "suspend" : null)}
        title={`Suspend ${displayName(person)}?`}
        description="Every request from this account is refused from now on, and its open connections close. Nothing is deleted; you can lift it later."
        confirmLabel="Suspend"
        errorAction="suspend this account"
        onConfirm={() => setSuspended(true)}
      >
        <FieldPlate
          label="Reason"
          htmlFor="admin-suspend-reason"
          ground="ply-3"
          hint="Optional"
        >
          <Textarea
            id="admin-suspend-reason"
            value={reason}
            maxLength={200}
            onChange={(event) => setReason(event.target.value)}
            placeholder="Shown here and in the audit log, not to them"
          />
        </FieldPlate>
      </ConfirmDialog>
      <ConfirmDialog
        open={confirm === "unsuspend"}
        onOpenChange={(open) => setConfirm(open ? "unsuspend" : null)}
        title={`Let ${displayName(person)} back in?`}
        description="Their sign-in works again on the next request."
        confirmLabel="Lift suspension"
        errorAction="lift the suspension"
        onConfirm={() => setSuspended(false)}
      />
      <ConfirmDialog
        open={confirm === "disconnect"}
        onOpenChange={(open) => setConfirm(open ? "disconnect" : null)}
        title="Close their live connections?"
        description="Their chat and voice sockets close and do not reconnect until the app reloads. It does not sign them out."
        confirmLabel="Close connections"
        errorAction="close the connections"
        onConfirm={disconnect}
      />
    </RuleCard>
  );
}

function DeviceList({
  detail,
  now,
}: {
  detail: AdminPersonDetail | null;
  now: number;
}): ReactNode {
  if (!detail) return null;
  return (
    <div className="flex flex-col gap-1.5">
      <p className="silkscreen text-ink-3">Devices with push</p>
      {detail.devices.length === 0 ? (
        <p className="text-ui-sm text-ink-3">
          No phone or desktop registered for push.
        </p>
      ) : (
        <ul className="flex flex-col gap-0.5">
          {detail.devices.map((device, index) => (
            <li key={index} className="flex h-8 items-center gap-2 text-ui-sm">
              <span className="min-w-0 flex-1 truncate text-ink-1">
                {device.platform}
                {device.app_variant ? (
                  <span className="text-ink-3"> · {device.app_variant}</span>
                ) : null}
              </span>
              <Num
                className="text-ink-3"
                title={formatDateTime(device.last_seen_at)}
              >
                {formatAgo(device.last_seen_at, now)}
              </Num>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
