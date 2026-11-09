/**
 * Live ops: the site right now. Who is online, the voice rooms as Talk
 * Tree branches with their people and streams, chunked uploads in flight,
 * D1 and R2, the deploy, the shop catalog, the cron sweeps and the
 * background job backlog. Refreshes every 10 s while the page is visible.
 */

import { useState } from "react";

import { Avatar } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Icon } from "@/components/ui/icon";
import { Led, LedLabel } from "@/components/ui/led";
import { Num, NumClock } from "@/components/ui/num";
import { RuleCard } from "@/components/ui/plate";
import { Skeleton } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { TreeBranch, TreeLeaf } from "@/components/ui/tree";
import { describeApiError } from "@/lib/api-errors";
import type {
  AdminCatalogRefreshResponse,
  AdminCatalogStatus,
  AdminLiveSnapshot,
  AdminVoiceRoom,
} from "../shared/admin-api";
import { assetUrl } from "./admin-format";
import { cn } from "@/lib/utils";

import { RefreshButton, SectionToolbar, useAdminShell } from "./AdminConsole";
import { useAdminResource, useAdminTransport } from "./admin-data";
import {
  formatAgo,
  formatBytes,
  formatCount,
  formatDateTime,
  shortCommit,
} from "./admin-format";
import {
  Freshness,
  LoadError,
  Readout,
  ReadoutRow,
  useAdminNow,
} from "./admin-ui";

const LIVE_REFRESH_MS = 10_000;
const REPO_COMMIT_URL = "https://github.com/115jon/yap/commit/";

export function LiveSection() {
  const now = useAdminNow();
  const live = useAdminResource<AdminLiveSnapshot>("/api/live", {
    refreshMs: LIVE_REFRESH_MS,
  });
  const data = live.data;

  return (
    <section
      aria-label="Live ops"
      className="flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <SectionToolbar>
        <LedLabel state={live.error ? "muted" : "talk"}>
          {live.error ? "Not live" : "Live"}
        </LedLabel>
        <p className="text-ui-sm text-ink-3 max-md:hidden">
          Refreshes every 5 seconds while this tab is open.
        </p>
        <div className="ml-auto flex items-center gap-3">
          <Freshness
            updatedAt={live.updatedAt}
            error={live.error}
            staleAfterMs={15_000}
            now={now}
          />
          <RefreshButton onRefresh={() => void live.refresh()} />
        </div>
      </SectionToolbar>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain custom-scrollbar">
        {live.loading && !data ? (
          <LiveSkeleton />
        ) : live.error && !data ? (
          <LoadError
            error={live.error}
            what="live state"
            onRetry={() => void live.refresh()}
          />
        ) : data ? (
          <LiveBody
            data={data}
            now={now}
            onChanged={() => void live.refresh()}
          />
        ) : null}
      </div>
    </section>
  );
}

function LiveSkeleton() {
  return (
    <div aria-hidden="true" className="flex flex-col gap-8 p-6 max-md:p-3">
      <Skeleton className="h-20 rounded-[2px]" />
      <div className="grid gap-8 lg:grid-cols-2">
        <Skeleton className="h-56 rounded-[2px]" />
        <Skeleton className="h-56 rounded-[2px]" />
      </div>
    </div>
  );
}

function LiveBody({
  data,
  now,
  onChanged,
}: {
  data: AdminLiveSnapshot;
  now: number;
  onChanged: () => void;
}) {
  const { openPerson } = useAdminShell();
  const inVoice =
    data.voice_rooms?.reduce((sum, room) => sum + room.members.length, 0) ??
    null;
  const streams =
    data.voice_rooms?.reduce(
      (sum, room) =>
        sum + room.members.filter((member) => member.self_stream).length,
      0,
    ) ?? null;
  const gatewayDown = data.gateway === null;

  return (
    <div className="mx-auto flex w-full max-w-[1280px] flex-col gap-10 px-6 pt-6 pb-12 max-md:gap-8 max-md:px-3 max-md:pt-4">
      {gatewayDown ? (
        <p className="flex items-center gap-2 rounded-[2px] bg-warning-tint px-3 py-2 text-ui-sm text-ink-1">
          <Led state="idle" />
          The global gateway didn&apos;t answer, so online and voice are
          unknown. D1 numbers are current.
        </p>
      ) : null}

      <ReadoutRow>
        <Readout
          label="Online"
          led={data.online && data.online.length > 0 ? "talk" : "off"}
          value={data.online ? data.online.length : "—"}
          detail="people with a chat socket"
        />
        <Readout
          label="In voice"
          led={inVoice ? "talk" : "off"}
          value={inVoice ?? "—"}
          detail={`${data.voice_rooms?.length ?? 0} rooms`}
        />
        <Readout
          label="Streaming"
          led={streams ? "live" : "off"}
          value={streams ?? "—"}
          detail="screens shared now"
        />
        <Readout
          label="Gateway sockets"
          value={data.gateway ? formatCount(data.gateway.sockets) : "—"}
          detail={
            data.gateway
              ? `${data.gateway.resumable_sessions} held for resume`
              : "gateway unavailable"
          }
        />
        <Readout
          label="Uploads in flight"
          led={data.pending_uploads.length ? "live" : "off"}
          value={data.pending_uploads.length}
          detail={
            formatBytes(
              data.pending_uploads.reduce(
                (sum, upload) => sum + upload.received_bytes,
                0,
              ),
            ) + " received"
          }
        />
      </ReadoutRow>

      <div className="grid gap-x-10 gap-y-10 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
        <RuleCard
          legend="Voice rooms"
          index={<Num>{data.voice_rooms?.length ?? "—"}</Num>}
        >
          {!data.voice_rooms ? (
            <p className="text-ui-sm text-ink-3">
              Unknown while the gateway is down.
            </p>
          ) : data.voice_rooms.length === 0 ? (
            <EmptyState
              size="sm"
              motif="voice"
              title="Nobody's in voice"
              description="Rooms show up here the moment someone joins."
            />
          ) : (
            <div className="flex flex-col gap-3">
              {data.voice_rooms.map((room) => (
                <VoiceRoomBranch
                  key={room.channel_id}
                  room={room}
                  now={now}
                  onOpen={openPerson}
                />
              ))}
            </div>
          )}
        </RuleCard>

        <RuleCard
          legend="Online now"
          index={<Num>{data.online?.length ?? "—"}</Num>}
        >
          {!data.online ? (
            <p className="text-ui-sm text-ink-3">
              Unknown while the gateway is down.
            </p>
          ) : data.online.length === 0 ? (
            <p className="text-ui-sm text-ink-3">Nobody is connected.</p>
          ) : (
            <ul className="flex flex-col gap-0.5">
              {data.online.map((user) => (
                <li key={user.user_id}>
                  <button
                    type="button"
                    onClick={() => openPerson(user.user_id)}
                    className="flex h-10 w-full min-w-0 items-center gap-2.5 rounded-[2px] px-2 text-left pointer-fine:hover:bg-tint-1"
                  >
                    <Avatar
                      src={user.avatar_url ? assetUrl(user.avatar_url) : null}
                      name={user.name}
                      size={24}
                      presence={
                        user.status === "idle"
                          ? "idle"
                          : user.status === "dnd"
                            ? "dnd"
                            : "online"
                      }
                      ring="ply-0"
                    />
                    <span className="min-w-0 flex-1 truncate text-ui-row text-ink-1">
                      {user.name}
                    </span>
                    {user.voice_channel_id ? (
                      <Icon
                        name="voiceChannel"
                        size="sm"
                        className="shrink-0 text-success-ink"
                      />
                    ) : null}
                    <span className="silkscreen shrink-0 text-ink-3">
                      {user.platforms.join(" · ") || "web"}
                    </span>
                    {user.sockets > 1 ? (
                      <Num className="shrink-0 text-ink-3">×{user.sockets}</Num>
                    ) : null}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </RuleCard>
      </div>

      <RuleCard
        legend="Chunked uploads in flight"
        index={<Num>{data.pending_uploads.length}</Num>}
      >
        {data.pending_uploads.length === 0 ? (
          <p className="text-ui-sm text-ink-3">No big file is on its way up.</p>
        ) : (
          <ul className="flex flex-col">
            {data.pending_uploads.map((upload) => {
              const share = upload.declared_size
                ? Math.min(1, upload.received_bytes / upload.declared_size)
                : 0;
              return (
                <li
                  key={upload.id}
                  className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1.5 border-b border-line py-2.5 last:border-b-0 md:grid-cols-[minmax(0,2fr)_minmax(0,1fr)_minmax(140px,1.2fr)_auto]"
                >
                  <span className="flex min-w-0 items-center gap-2">
                    <Icon
                      name="file"
                      size="sm"
                      className="shrink-0 text-ink-3"
                    />
                    <span className="truncate text-ui-row text-ink-1">
                      {upload.filename}
                    </span>
                  </span>
                  <span className="truncate text-ui-sm text-ink-2 max-md:order-3">
                    {upload.user_name ?? upload.user_id}
                  </span>
                  <span className="flex items-center gap-2 max-md:order-4 max-md:col-span-2">
                    <span
                      className="relative h-1 flex-1 overflow-hidden rounded-[1px] bg-tint-3"
                      role="progressbar"
                      aria-label={`${upload.filename} received`}
                      aria-valuemin={0}
                      aria-valuemax={100}
                      aria-valuenow={Math.round(share * 100)}
                    >
                      <span
                        className={cn(
                          "absolute inset-y-0 left-0 origin-left",
                          upload.status === "completing"
                            ? "bg-talk"
                            : "bg-live",
                        )}
                        style={{ width: `${share * 100}%` }}
                      />
                    </span>
                    <Num className="shrink-0 text-ink-2">
                      {formatBytes(upload.received_bytes)} /{" "}
                      {formatBytes(upload.declared_size)}
                    </Num>
                  </span>
                  <span className="justify-self-end max-md:order-2">
                    {upload.status === "completing" ? (
                      <LedLabel state="talk">Completing</LedLabel>
                    ) : (
                      <span className="text-num-xs text-ink-3">
                        {upload.parts_received}/{upload.part_count} parts ·{" "}
                        {formatAgo(upload.last_activity_at, now)}
                      </span>
                    )}
                  </span>
                </li>
              );
            })}
          </ul>
        )}
      </RuleCard>

      <div className="grid gap-x-10 gap-y-10 lg:grid-cols-3">
        <RuleCard
          legend="D1"
          index={
            <Num>
              {data.d1.size_bytes !== null
                ? formatBytes(data.d1.size_bytes)
                : "—"}
            </Num>
          }
        >
          <ul className="flex flex-col">
            {data.d1.tables.slice(0, 12).map((table) => (
              <li
                key={table.name}
                className="flex h-8 items-center gap-3 border-b border-line last:border-b-0"
              >
                <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-ink-2">
                  {table.name}
                </span>
                <Num className="text-ink-1">{formatCount(table.rows)}</Num>
              </li>
            ))}
          </ul>
          {data.d1.tables.length > 12 ? (
            <p className="mt-2 text-ui-xs text-ink-3">
              {data.d1.tables.length - 12} smaller tables hidden.
            </p>
          ) : null}
        </RuleCard>

        <RuleCard
          legend="R2 by kind"
          index={
            <Num>
              {formatBytes(
                data.storage.reduce((sum, kind) => sum + (kind.bytes ?? 0), 0),
              )}
            </Num>
          }
        >
          <StorageBars data={data} />
          <p className="mt-3 text-ui-xs text-ink-3">
            From the sizes D1 records for each object; avatars keep no size.
          </p>
        </RuleCard>

        <div className="flex flex-col gap-10">
          <RuleCard legend="Deployed">
            <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-ui-sm">
              <dt className="silkscreen text-ink-3">Version</dt>
              <dd>
                <Num size="sm" className="text-ink-1">
                  {data.deploy.version}
                </Num>
              </dd>
              <dt className="silkscreen text-ink-3">Commit</dt>
              <dd className="min-w-0 truncate">
                {/^[0-9a-f]{7,40}$/i.test(data.deploy.commit) ? (
                  <a
                    href={`${REPO_COMMIT_URL}${data.deploy.commit}`}
                    target="_blank"
                    rel="noreferrer"
                    className="font-mono text-[12px] text-accent-ink underline-offset-[3px] pointer-fine:hover:underline"
                  >
                    {shortCommit(data.deploy.commit)}
                  </a>
                ) : (
                  <span className="text-ink-3">{data.deploy.commit}</span>
                )}
              </dd>
              <dt className="silkscreen text-ink-3">Built</dt>
              <dd className="text-num-xs text-ink-2">
                {data.deploy.built_at
                  ? formatAgo(data.deploy.built_at, now)
                  : "—"}
              </dd>
            </dl>
          </RuleCard>

          <RuleCard legend="Background jobs">
            <dl className="grid grid-cols-[minmax(0,1fr)_auto] gap-x-4 gap-y-2 text-ui-sm">
              <dt className="text-ink-2">Embeds waiting to retry</dt>
              <dd>
                <Num
                  className={
                    data.jobs.postprocessing_pending
                      ? "text-warning-ink"
                      : "text-ink-1"
                  }
                >
                  {data.jobs.postprocessing_pending}
                </Num>
              </dd>
              <dt className="text-ink-2">R2 deletes waiting</dt>
              <dd>
                <Num
                  className={
                    data.jobs.r2_cleanup_pending
                      ? "text-warning-ink"
                      : "text-ink-1"
                  }
                >
                  {data.jobs.r2_cleanup_pending}
                </Num>
              </dd>
            </dl>
            {data.jobs.r2_cleanup_last_error ? (
              <p
                className="mt-2 truncate font-mono text-[12px] text-danger-ink"
                title={data.jobs.r2_cleanup_last_error}
              >
                {data.jobs.r2_cleanup_last_error}
              </p>
            ) : null}
          </RuleCard>
        </div>
      </div>

      <ShopCatalogCard
        catalog={data.catalog}
        generatedAt={data.generated_at}
        now={now}
        onChanged={onChanged}
      />

      <RuleCard legend="Cron sweeps" index={<Num>{data.cron.length}</Num>}>
        {data.cron.length === 0 ? (
          <p className="text-ui-sm text-ink-3">
            No sweep has run since this was deployed. They run on the hour.
          </p>
        ) : (
          <ul className="grid gap-x-10 md:grid-cols-2">
            {data.cron.map((run) => (
              <li
                key={run.name}
                className="flex min-h-10 items-center gap-3 border-b border-line py-1.5"
              >
                <LedLabel
                  state={run.ok ? "talk" : "muted"}
                  className="w-16 shrink-0"
                >
                  {run.ok ? "OK" : "Failed"}
                </LedLabel>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-ui-row text-ink-1">
                    {run.name}
                  </span>
                  {run.error ? (
                    <span
                      className="truncate text-ui-xs text-danger-ink"
                      title={run.error}
                    >
                      {run.error}
                    </span>
                  ) : null}
                </span>
                <span className="flex shrink-0 flex-col items-end">
                  <Num
                    className="text-ink-2"
                    title={formatDateTime(run.ran_at)}
                  >
                    {formatAgo(run.ran_at, now)}
                  </Num>
                  <Num className="text-ink-3">
                    {(run.duration_ms / 1000).toFixed(1)} s
                  </Num>
                </span>
              </li>
            ))}
          </ul>
        )}
      </RuleCard>

      <p className="text-ui-xs text-ink-3">
        Server errors are not listed: nothing in Yap stores them yet. They are
        in Workers logs (`pnpm logs`). Room sockets other than the global
        gateway are counted by their voice members, not one by one.
      </p>
    </div>
  );
}

const OUTCOME_WORD: Record<
  NonNullable<AdminCatalogStatus["last_outcome"]>,
  string
> = {
  updated: "Updated",
  not_modified: "Unchanged",
  rejected: "Rejected",
  failed: "Failed",
};

/**
 * The collectibles shop catalog: how fresh it is, the last refresh and its
 * error, the off-Cloudflare fetcher's last push, and "Refresh now".
 */
function ShopCatalogCard({
  catalog,
  generatedAt,
  now,
  onChanged,
}: {
  catalog: AdminCatalogStatus | null;
  generatedAt: string;
  now: number;
  onChanged: () => void;
}) {
  const transport = useAdminTransport();
  const [busy, setBusy] = useState(false);

  if (!catalog) {
    return (
      <RuleCard legend="Shop catalog">
        <p className="text-ui-sm text-ink-3">
          The catalog&apos;s status couldn&apos;t be read.
        </p>
      </RuleCard>
    );
  }

  const failing =
    catalog.last_outcome === "failed" || catalog.last_outcome === "rejected";
  const waitMs = Math.max(
    0,
    Date.parse(generatedAt) + catalog.retry_after_ms - now,
  );

  const refresh = async () => {
    setBusy(true);
    try {
      const result = await transport.send<AdminCatalogRefreshResponse>(
        "POST",
        "/api/catalog/refresh",
      );
      toast.success(
        `Catalog refreshed: ${formatCount(result.catalog.items)} items`,
      );
    } catch (error) {
      toast.error(describeApiError(error, "refresh the catalog"));
    } finally {
      setBusy(false);
      onChanged();
    }
  };

  return (
    <RuleCard
      legend="Shop catalog"
      index={
        catalog.stale ? (
          <LedLabel state="muted">Stale</LedLabel>
        ) : (
          <LedLabel state="talk">Current</LedLabel>
        )
      }
    >
      <div className="flex flex-wrap items-start gap-x-10 gap-y-4">
        <dl className="grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-ui-sm">
          <dt className="silkscreen text-ink-3">Synced</dt>
          <dd>
            <Num
              className={catalog.stale ? "text-danger-ink" : "text-ink-1"}
              title={
                catalog.synced_at
                  ? formatDateTime(catalog.synced_at)
                  : undefined
              }
            >
              {catalog.synced_at ? formatAgo(catalog.synced_at, now) : "Never"}
            </Num>
          </dd>
          <dt className="silkscreen text-ink-3">Items</dt>
          <dd>
            <Num className="text-ink-1">{formatCount(catalog.items)}</Num>{" "}
            <span className="text-ink-3">
              in {formatCount(catalog.categories)} collections
            </span>
          </dd>
          <dt className="silkscreen text-ink-3">Last try</dt>
          <dd className="flex min-w-0 items-center gap-2">
            {catalog.last_outcome ? (
              <>
                <LedLabel state={failing ? "muted" : "talk"}>
                  {OUTCOME_WORD[catalog.last_outcome]}
                </LedLabel>
                <span className="truncate text-ink-3">
                  {catalog.last_trigger ?? "unknown"}
                  {catalog.last_attempt_at
                    ? `, ${formatAgo(catalog.last_attempt_at, now)}`
                    : ""}
                  {catalog.consecutive_failures > 1
                    ? `, ${formatCount(catalog.consecutive_failures)} in a row`
                    : ""}
                </span>
              </>
            ) : (
              <span className="text-ink-3">Never</span>
            )}
          </dd>
          <dt className="silkscreen text-ink-3">Fetcher</dt>
          <dd className="text-ink-2">
            {catalog.fetcher.last_success_at ? (
              <span title={formatDateTime(catalog.fetcher.last_success_at)}>
                Pushed {formatAgo(catalog.fetcher.last_success_at, now)}
              </span>
            ) : (
              "Never pushed"
            )}
          </dd>
        </dl>
        <div className="flex shrink-0 flex-col items-end gap-1">
          <Button
            variant="secondary"
            loading={busy}
            disabled={busy || waitMs > 0}
            onClick={() => void refresh()}
          >
            Refresh now
          </Button>
          {waitMs > 0 ? (
            <span className="text-ui-xs text-ink-3">
              Ready in {Math.ceil(waitMs / 1000)} s
            </span>
          ) : null}
        </div>
      </div>
      {failing && catalog.last_error ? (
        <p
          className="mt-3 line-clamp-2 font-mono text-[12px] text-danger-ink"
          title={catalog.last_error}
        >
          {catalog.last_error}
        </p>
      ) : null}
      {catalog.fetcher.last_error ? (
        <p
          className="mt-2 truncate font-mono text-[12px] text-danger-ink"
          title={catalog.fetcher.last_error}
        >
          Fetcher: {catalog.fetcher.last_error}
        </p>
      ) : null}
      {failing && catalog.last_error?.includes(" 403 ") ? (
        <p className="mt-3 text-ui-xs text-pretty text-ink-3">
          Upstream is refusing Cloudflare, so the hourly cron and Refresh now
          fail. The off-Cloudflare fetcher keeps the shop current; when it stops
          pushing, the catalog goes stale here.
        </p>
      ) : null}
    </RuleCard>
  );
}

function VoiceRoomBranch({
  room,
  now,
  onOpen,
}: {
  room: AdminVoiceRoom;
  now: number;
  onOpen: (userId: string) => void;
}) {
  const streams = room.members.filter((member) => member.self_stream).length;
  return (
    <TreeBranch
      rowHeight={32}
      guideX={16}
      header={
        <div className="flex h-8 items-center gap-2 px-2">
          <Icon
            name={room.kind === "dm" ? "call" : "voiceChannel"}
            size="md"
            className="shrink-0 text-ink-2"
          />
          <span className="min-w-0 truncate text-ui-row font-semibold text-ink-1">
            {room.channel_name ?? "Unknown channel"}
          </span>
          <span className="min-w-0 truncate text-ui-xs text-ink-3">
            {room.kind === "dm" ? "DM call" : room.server_name}
          </span>
          <span className="ml-auto flex shrink-0 items-center gap-3">
            {streams ? <LedLabel state="live">{streams} live</LedLabel> : null}
            {room.started_at ? (
              <NumClock
                since={room.started_at}
                now={now}
                className="text-ink-3"
              />
            ) : null}
          </span>
        </div>
      }
    >
      {room.members.map((member) => (
        <TreeLeaf
          key={member.user_id}
          node={
            <Led
              state={
                member.connection_state === "reconnecting"
                  ? "idle"
                  : member.self_mute || member.self_deaf
                    ? "muted"
                    : "talk"
              }
            />
          }
          rowClassName="gap-2 pr-2"
        >
          <button
            type="button"
            onClick={() => onOpen(member.user_id)}
            className="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-[2px] px-1 text-left pointer-fine:hover:bg-tint-1"
          >
            <Avatar
              src={member.avatar_url ? assetUrl(member.avatar_url) : null}
              name={member.name}
              size={20}
              ring="ply-0"
            />
            <span className="min-w-0 truncate text-ui-sm text-ink-1">
              {member.name}
            </span>
            <span className="ml-auto flex shrink-0 items-center gap-1.5 text-ink-3">
              {member.connection_state === "reconnecting" ? (
                <span className="silkscreen text-warning-ink">
                  Reconnecting
                </span>
              ) : null}
              {member.self_deaf ? (
                <Icon
                  name="deafenOff"
                  size="sm"
                  className="text-danger-ink"
                  aria-label="Deafened"
                />
              ) : member.self_mute ? (
                <Icon
                  name="micOff"
                  size="sm"
                  className="text-danger-ink"
                  aria-label="Muted"
                />
              ) : null}
              {member.self_video ? (
                <Icon
                  name="camera"
                  size="sm"
                  className="text-live-ink"
                  aria-label="Camera on"
                />
              ) : null}
              {member.self_stream ? (
                <Icon
                  name="screenShare"
                  size="sm"
                  className="text-live-ink"
                  aria-label="Sharing a screen"
                />
              ) : null}
            </span>
          </button>
        </TreeLeaf>
      ))}
    </TreeBranch>
  );
}

function StorageBars({ data }: { data: AdminLiveSnapshot }) {
  const max = Math.max(1, ...data.storage.map((kind) => kind.bytes ?? 0));
  return (
    <ul className="flex flex-col gap-3">
      {data.storage.map((kind) => (
        <li key={kind.kind} className="flex flex-col gap-1">
          <span className="flex items-baseline gap-3">
            <span className="min-w-0 flex-1 truncate text-ui-sm text-ink-1">
              {kind.kind}
            </span>
            <Num className="text-ink-3">{formatCount(kind.objects)}</Num>
            <Num className="w-16 text-right text-ink-1">
              {kind.bytes === null ? "—" : formatBytes(kind.bytes)}
            </Num>
          </span>
          <span aria-hidden="true" className="h-1 rounded-[1px] bg-tint-2">
            <span
              className="block h-full rounded-[1px] bg-ink-2"
              style={{ width: `${((kind.bytes ?? 0) / max) * 100}%` }}
            />
          </span>
        </li>
      ))}
    </ul>
  );
}
