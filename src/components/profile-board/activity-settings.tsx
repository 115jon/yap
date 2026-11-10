/**
 * "Who sees your activity": the owner's privacy rows under the timeline.
 * One switch per kind ("shown to others"; off keeps that kind to the owner)
 * and the retention rail. Saving is the parent's job (optimistic, reverts on
 * error); this only renders the settings it is given.
 */

import * as React from "react";

import { SegmentedControl } from "@/components/ui/segmented-control";
import { Switch } from "@/components/ui/switch";
import {
  ACTIVITY_KINDS,
  ACTIVITY_RETENTION_DAYS,
  type ActivityKind,
  type ActivityRetentionDays,
  type ActivitySettings,
} from "@/lib/profile-board";
import { cn } from "@/lib/utils";

import { ACTIVITY_KIND_PRIVACY_COPY } from "./activity-format";

const RETENTION_ITEMS = ACTIVITY_RETENTION_DAYS.map((days) => ({
  value: String(days) as `${ActivityRetentionDays}`,
  label: `${days} days`,
}));

export function ActivitySettingsSection({
  settings,
  error,
  onToggleKind,
  onRetention,
  className,
  ref,
}: {
  settings: ActivitySettings;
  /** The last save's failure, already in words. */
  error: string | null;
  onToggleKind: (kind: ActivityKind, shown: boolean) => void;
  onRetention: (days: ActivityRetentionDays) => void;
  className?: string;
  ref?: React.Ref<HTMLElement>;
}) {
  const id = React.useId();
  const shownCount = ACTIVITY_KINDS.length - settings.hidden_kinds.length;
  return (
    <section
      ref={ref}
      aria-labelledby={`${id}-title`}
      tabIndex={-1}
      className={cn(
        "@container border-t-2 border-ink-1 pt-3 outline-none",
        className,
      )}
    >
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <h3
          id={`${id}-title`}
          className="font-display text-[16px] leading-5 font-bold text-balance text-ink-1"
        >
          Who sees your activity
        </h3>
        <p className="text-ui-xs text-ink-3">
          Others see{" "}
          <span className="text-num-xs text-ink-2">
            {shownCount} of {ACTIVITY_KINDS.length}
          </span>{" "}
          kinds
        </p>
      </div>
      <p className="mt-1 max-w-[60ch] text-ui-sm text-pretty text-ink-2">
        You always see everything here. Switch a kind off to keep it to
        yourself, or hide one event with its eye.
      </p>

      {error ? (
        <p
          role="alert"
          className="mt-3 rounded-[2px] bg-danger-tint px-3 py-2 text-ui-sm text-danger-ink"
        >
          {error}
        </p>
      ) : null}

      <div className="mt-2 flex flex-col gap-x-6 @lg:grid @lg:grid-cols-2">
        {ACTIVITY_KINDS.map((kind) => {
          const copy = ACTIVITY_KIND_PRIVACY_COPY[kind];
          const shown = !settings.hidden_kinds.includes(kind);
          const labelId = `${id}-${kind}`;
          return (
            <div
              key={kind}
              className="flex min-h-12 items-center justify-between gap-3 border-b border-line py-2"
            >
              <span className="min-w-0">
                <span
                  id={labelId}
                  className="block text-ui-row font-medium text-ink-1"
                >
                  {copy.label}
                </span>
                <span className="block text-ui-xs text-ink-3">
                  {shown ? "Shown to others" : "Only you"}
                  {copy.detail ? ` · ${copy.detail}` : ""}
                </span>
              </span>
              <Switch
                size="sm"
                checked={shown}
                aria-labelledby={labelId}
                onCheckedChange={(next) => onToggleKind(kind, next)}
              />
            </div>
          );
        })}
      </div>

      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 py-3">
        <span className="min-w-0">
          <span
            id={`${id}-retention`}
            className="block text-ui-row font-medium text-ink-1"
          >
            Keep activity for
          </span>
          <span className="block text-ui-xs text-ink-3">
            Older events are deleted, not just hidden
          </span>
        </span>
        <SegmentedControl
          label="Keep activity for"
          size="sm"
          items={RETENTION_ITEMS}
          value={String(settings.retention_days) as `${ActivityRetentionDays}`}
          onValueChange={(value) =>
            onRetention(Number(value) as ActivityRetentionDays)
          }
        />
      </div>
    </section>
  );
}
