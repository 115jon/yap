/**
 * Insights: how Yap is used, by UTC day, over 7, 30 or 90 days. Daily
 * actives, messages, voice and stream hours, sign-ups, uploads (count and
 * volume as two charts, never two axes) and the busiest servers.
 *
 * Messages, sign-ups and uploads come straight from their tables. Voice and
 * stream hours (and the voice half of daily actives) come from the
 * daily_activity rollup, which starts when the console shipped; the charts
 * say so when a range reaches before it.
 */

import { useState } from "react";

import { EmptyState } from "@/components/ui/empty-state";
import { RuleCard } from "@/components/ui/plate";
import { SegmentedControl } from "@/components/ui/segmented-control";
import { Skeleton } from "@/components/ui/skeleton";
import type { AdminInsights } from "../shared/admin-api";

import { RefreshButton, SectionToolbar } from "./AdminConsole";
import { DailyBars, RankedBars } from "./AdminCharts";
import { useAdminResource } from "./admin-data";
import {
  formatBytes,
  formatCount,
  formatDate,
  formatDay,
} from "./admin-format";
import {
  Freshness,
  LoadError,
  Readout,
  ReadoutRow,
  useAdminNow,
} from "./admin-ui";

type Range = "7" | "30" | "90";

const hours = (value: number) =>
  value >= 10 ? `${Math.round(value)} h` : `${value.toFixed(1)} h`;
const count = (value: number) => formatCount(value);
const sum = (points: { value: number }[]) =>
  points.reduce((total, point) => total + point.value, 0);

export default function InsightsSection() {
  const now = useAdminNow();
  const [range, setRange] = useState<Range>("30");
  const insights = useAdminResource<AdminInsights>(
    `/api/insights?days=${range}`,
    {
      refreshMs: 5 * 60_000,
    },
  );
  const data = insights.data;

  return (
    <section
      aria-label="Insights"
      className="flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <SectionToolbar>
        <SegmentedControl
          label="Range"
          size="sm"
          items={[
            { value: "7", label: "7 days" },
            { value: "30", label: "30 days" },
            { value: "90", label: "90 days" },
          ]}
          value={range}
          onValueChange={setRange}
          className="w-[300px] max-md:w-full"
        />
        {data ? (
          <p className="text-num-xs text-ink-3 max-md:hidden">
            {formatDay(data.from)} – {formatDay(data.to)} · UTC
          </p>
        ) : null}
        <div className="ml-auto flex items-center gap-3 max-md:hidden">
          <Freshness
            updatedAt={insights.updatedAt}
            error={insights.error}
            staleAfterMs={10 * 60_000}
            now={now}
          />
          <RefreshButton onRefresh={() => void insights.refresh()} />
        </div>
      </SectionToolbar>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain custom-scrollbar">
        {insights.loading && !data ? (
          <InsightsSkeleton />
        ) : insights.error && !data ? (
          <LoadError
            error={insights.error}
            what="insights"
            onRetry={() => void insights.refresh()}
          />
        ) : data ? (
          <InsightsBody data={data} />
        ) : null}
      </div>
    </section>
  );
}

function InsightsSkeleton() {
  return (
    <div
      aria-hidden="true"
      className="grid gap-10 p-6 md:grid-cols-2 max-md:p-3"
    >
      {Array.from({ length: 6 }, (_, index) => (
        <Skeleton key={index} className="h-48 rounded-[2px]" />
      ))}
    </div>
  );
}

function InsightsBody({ data }: { data: AdminInsights }) {
  const partial = data.rollup_since === null || data.rollup_since > data.from;
  const rollupNote = partial
    ? data.rollup_since
      ? `Counted from ${formatDate(data.rollup_since)}, when the rollup started.`
      : "Counted from when the rollup started; nothing is recorded yet."
    : undefined;
  const peakActives = Math.max(
    0,
    ...data.daily_actives.map((point) => point.value),
  );

  return (
    <div className="mx-auto flex w-full max-w-[1280px] flex-col gap-10 px-6 pt-6 pb-12 max-md:gap-8 max-md:px-3 max-md:pt-4">
      <ReadoutRow>
        <Readout
          label="People"
          value={formatCount(data.totals.people)}
          detail="accounts in all"
        />
        <Readout
          label="Servers"
          value={formatCount(data.totals.servers)}
          detail="in all"
        />
        <Readout
          label="Messages"
          value={formatCount(sum(data.messages))}
          detail={`in ${data.days} days · ${formatCount(data.totals.messages)} ever`}
        />
        <Readout
          label="Peak actives"
          value={peakActives}
          detail="people on the busiest day"
        />
        <Readout
          label="Voice"
          value={hours(sum(data.voice_hours))}
          detail={`in ${data.days} days`}
        />
      </ReadoutRow>

      <div className="grid gap-x-12 gap-y-10 md:grid-cols-2">
        <DailyBars
          title="Daily actives"
          points={data.daily_actives}
          format={count}
          headlineLabel="Today"
          headline={count(data.daily_actives.at(-1)?.value ?? 0)}
          note={
            partial
              ? "Before the rollup started, only people who wrote a message count."
              : undefined
          }
        />
        <DailyBars
          title="Messages"
          points={data.messages}
          format={count}
          headlineLabel="Total"
          headline={count(sum(data.messages))}
        />
        <DailyBars
          title="Voice hours"
          points={data.voice_hours}
          format={hours}
          headlineLabel="Total"
          headline={hours(sum(data.voice_hours))}
          note={rollupNote}
        />
        <DailyBars
          title="Stream hours"
          points={data.stream_hours}
          format={hours}
          headlineLabel="Total"
          headline={hours(sum(data.stream_hours))}
          note={rollupNote}
        />
        <DailyBars
          title="Sign-ups"
          points={data.signups}
          format={count}
          headlineLabel="Total"
          headline={count(sum(data.signups))}
        />
        <DailyBars
          title="Uploads"
          points={data.uploads}
          format={count}
          headlineLabel="Total"
          headline={count(sum(data.uploads))}
        />
        <DailyBars
          title="Upload volume"
          points={data.upload_bytes}
          format={formatBytes}
          headlineLabel="Total"
          headline={formatBytes(sum(data.upload_bytes))}
        />
        <RuleCard legend="Top servers" index={`${data.days} days`}>
          {data.top_servers.length === 0 ? (
            <EmptyState
              size="sm"
              title="No server messages yet"
              description="Servers rank by messages written in the range."
            />
          ) : (
            <RankedBars
              rows={data.top_servers.map((server) => ({
                id: server.id,
                label: server.name,
                value: server.messages,
                detail: `${server.active_members} of ${server.members} active`,
              }))}
              format={count}
            />
          )}
        </RuleCard>
      </div>
    </div>
  );
}
