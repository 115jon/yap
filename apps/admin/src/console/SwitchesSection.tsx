/**
 * Switches: the site's kill switches, the announcement banner and the
 * desktop native handoff. Each kill switch is wired into its real server
 * path (services/site-flags.service), so off means the path refuses with 503
 * FEATURE_PAUSED and a sentence the clients show. Turning one off asks first;
 * turning one on does not. The handoff is the other way round: it starts
 * off, turning it on asks first, and off stops it at once.
 */

import { useEffect, useState } from "react";

import { SiteAnnouncementBar } from "@/components/chat/SiteAnnouncementBar";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { FieldPlate, Input } from "@/components/ui/input";
import { LedLabel } from "@/components/ui/led";
import { Num } from "@/components/ui/num";
import { RuleCard } from "@/components/ui/plate";
import { SegmentedControl } from "@/components/ui/segmented-control";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { toast } from "@/components/ui/toast";
import type { AdminSettingsResponse } from "../shared/admin-api";
import { describeApiError } from "@/lib/api-errors";
import {
  parseRolloutPercent,
  type NativeHandoffConfig,
} from "@/lib/native-handoff";
import {
  activeAnnouncement,
  ANNOUNCEMENT_MAX_LENGTH,
  SITE_FLAG_INFO,
  SITE_FLAGS,
  type AnnouncementTone,
  type SiteAnnouncement,
  type SiteFlag,
} from "@/lib/site-flags";

import { RefreshButton, SectionToolbar } from "./AdminConsole";
import { useAdminResource, useAdminTransport } from "./admin-data";
import { formatAgo, formatDateTime, formatUntil } from "./admin-format";
import { Freshness, LoadError, useAdminNow } from "./admin-ui";

export function SwitchesSection() {
  const now = useAdminNow();
  const settings = useAdminResource<AdminSettingsResponse>("/api/settings", {
    refreshMs: 30_000,
  });
  return (
    <section
      aria-label="Switches"
      className="flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <SectionToolbar>
        <p className="text-ui-sm text-ink-2">
          A switch takes effect within 10 seconds everywhere. Every change is
          logged.
        </p>
        <div className="ml-auto flex items-center gap-3 max-md:hidden">
          <Freshness
            updatedAt={settings.updatedAt}
            error={settings.error}
            staleAfterMs={60_000}
            now={now}
          />
          <RefreshButton onRefresh={() => void settings.refresh()} />
        </div>
      </SectionToolbar>
      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain custom-scrollbar">
        {settings.loading && !settings.data ? (
          <div
            aria-hidden="true"
            className="flex flex-col gap-6 p-6 max-md:p-3"
          >
            <Skeleton className="h-72 rounded-[2px]" />
            <Skeleton className="h-64 rounded-[2px]" />
          </div>
        ) : settings.error && !settings.data ? (
          <LoadError
            error={settings.error}
            what="the switches"
            onRetry={() => void settings.refresh()}
          />
        ) : settings.data ? (
          <div className="mx-auto grid w-full max-w-[1180px] gap-x-12 gap-y-10 px-6 pt-6 pb-12 max-md:px-3 max-md:pt-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
            <FlagsCard settings={settings} now={now} />
            <AnnouncementCard settings={settings} now={now} />
            <NativeHandoffCard settings={settings} now={now} />
          </div>
        ) : null}
      </div>
    </section>
  );
}

type SettingsResource = ReturnType<
  typeof useAdminResource<AdminSettingsResponse>
>;

function FlagsCard({
  settings,
  now,
}: {
  settings: SettingsResource;
  now: number;
}) {
  const transport = useAdminTransport();
  const data = settings.data!;
  const [pending, setPending] = useState<SiteFlag | null>(null);
  const [busy, setBusy] = useState<SiteFlag | null>(null);
  const offCount = SITE_FLAGS.filter((flag) => !data.flags[flag]).length;

  const write = async (flag: SiteFlag, on: boolean) => {
    const previous = data;
    settings.mutate((current) =>
      current
        ? { ...current, flags: { ...current.flags, [flag]: on } }
        : current,
    );
    setBusy(flag);
    try {
      const next = await transport.send<AdminSettingsResponse>(
        "PUT",
        "/api/settings",
        {
          flags: { [flag]: on },
        },
      );
      settings.mutate(() => next);
      toast.success(`${SITE_FLAG_INFO[flag].label} ${on ? "on" : "off"}`);
    } catch (error) {
      settings.mutate(() => previous);
      throw error;
    } finally {
      setBusy(null);
    }
  };

  return (
    <RuleCard
      legend="Kill switches"
      index={
        offCount ? (
          <LedLabel state="idle">{offCount} off</LedLabel>
        ) : (
          <LedLabel state="talk">All on</LedLabel>
        )
      }
      title="Features"
    >
      <ul className="flex flex-col">
        {SITE_FLAGS.map((flag) => {
          const info = SITE_FLAG_INFO[flag];
          const on = data.flags[flag];
          const labelId = `admin-flag-${flag}`;
          return (
            <li
              key={flag}
              className="flex min-h-16 items-start gap-4 border-b border-line py-3 last:border-b-0"
            >
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <span className="flex items-center gap-3">
                  <span
                    id={labelId}
                    className="text-ui-base font-medium text-ink-1"
                  >
                    {info.label}
                  </span>
                  {on ? null : <LedLabel state="muted">Off</LedLabel>}
                </span>
                <span className="text-ui-sm text-pretty text-ink-2">
                  {info.description}
                </span>
                {!on ? (
                  <span className="text-ui-xs text-ink-3">
                    People see: “{info.pausedMessage}”
                  </span>
                ) : null}
              </div>
              <Switch
                aria-labelledby={labelId}
                checked={on}
                disabled={busy === flag}
                onCheckedChange={(checked) => {
                  if (checked) {
                    void write(flag, true).catch((error) =>
                      toast.error(
                        describeApiError(
                          error,
                          `turn ${info.label.toLowerCase()} on`,
                        ),
                      ),
                    );
                  } else {
                    setPending(flag);
                  }
                }}
                className="mt-0.5"
              />
            </li>
          );
        })}
      </ul>
      {data.updated.flags ? (
        <p
          className="mt-3 text-ui-xs text-ink-3"
          title={formatDateTime(data.updated.flags.at)}
        >
          Last changed {formatAgo(data.updated.flags.at, now)}.
        </p>
      ) : null}
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(open) => {
          if (!open) setPending(null);
        }}
        title={
          pending
            ? `Turn off ${SITE_FLAG_INFO[pending].label.toLowerCase()}?`
            : ""
        }
        description={
          pending ? (
            <>
              {SITE_FLAG_INFO[pending].description} Everyone who tries is told:
              “{SITE_FLAG_INFO[pending].pausedMessage}”
            </>
          ) : null
        }
        confirmLabel="Turn off"
        errorAction="turn it off"
        onConfirm={() => (pending ? write(pending, false) : undefined)}
      />
    </RuleCard>
  );
}

function NativeHandoffCard({
  settings,
  now,
}: {
  settings: SettingsResource;
  now: number;
}) {
  const transport = useAdminTransport();
  const data = settings.data!;
  const current = data.nativeHandoff;
  const [percentText, setPercentText] = useState(
    String(current.rolloutPercent),
  );
  const [confirming, setConfirming] = useState(false);
  // A raise while the handoff is on moves more installs: asked first.
  const [raiseTo, setRaiseTo] = useState<number | null>(null);
  const [busy, setBusy] = useState<"switch" | "rollout" | null>(null);
  const [error, setError] = useState<string | null>(null);

  // A save elsewhere (or the first load) resets the field to what is stored.
  useEffect(() => {
    setPercentText(String(current.rolloutPercent));
  }, [current.rolloutPercent]);

  const percent = parseRolloutPercent(
    percentText.trim() === "" ? Number.NaN : Number(percentText),
  );
  const percentChanged = percent !== null && percent !== current.rolloutPercent;

  const write = async (
    patch: Partial<NativeHandoffConfig>,
    what: "switch" | "rollout",
  ) => {
    const previous = data;
    settings.mutate((value) =>
      value
        ? { ...value, nativeHandoff: { ...value.nativeHandoff, ...patch } }
        : value,
    );
    setBusy(what);
    setError(null);
    try {
      const next = await transport.send<AdminSettingsResponse>(
        "PUT",
        "/api/settings",
        { nativeHandoff: patch },
      );
      settings.mutate(() => next);
      toast.success(
        what === "switch"
          ? `Native handoff ${patch.enabled ? "on" : "off"}`
          : `Rollout set to ${patch.rolloutPercent}%`,
      );
    } catch (err) {
      settings.mutate(() => previous);
      throw err;
    } finally {
      setBusy(null);
    }
  };

  return (
    <RuleCard
      legend="Desktop"
      index={
        current.enabled ? (
          <LedLabel state={current.rolloutPercent > 0 ? "talk" : "idle"}>
            On · {current.rolloutPercent}%
          </LedLabel>
        ) : (
          <LedLabel state="off">Off</LedLabel>
        )
      }
      title="Native app handoff"
    >
      <div className="flex flex-col gap-5">
        <p className="text-ui-sm text-pretty text-ink-2">
          The Tauri desktop app (3.14) downloads the native app from the latest
          GitHub release, checks its signature, installs it and closes. The
          native app then imports its settings and uninstalls it. Nothing
          happens until the latest release is a native one (v4 or later).
        </p>
        <div className="flex min-h-16 items-start gap-4 border-b border-line pb-3">
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <span
              id="admin-native-handoff-label"
              className="text-ui-base font-medium text-ink-1"
            >
              Hand off to the native app
            </span>
            <span className="text-ui-sm text-pretty text-ink-2">
              Off stops new handoffs within a few minutes. Installs that already
              moved stay moved.
            </span>
          </div>
          <Switch
            aria-labelledby="admin-native-handoff-label"
            checked={current.enabled}
            disabled={busy !== null}
            onCheckedChange={(checked) => {
              if (checked) {
                setConfirming(true);
              } else {
                void write({ enabled: false }, "switch").catch((err) =>
                  toast.error(describeApiError(err, "turn the handoff off")),
                );
              }
            }}
            className="mt-0.5"
          />
        </div>
        <FieldPlate
          label="Rollout"
          htmlFor="admin-native-handoff-rollout"
          hint="Percent of installs, 0–100"
        >
          <div className="flex items-center gap-3">
            <Input
              id="admin-native-handoff-rollout"
              type="number"
              inputMode="numeric"
              min={0}
              max={100}
              step={1}
              value={percentText}
              onChange={(event) => setPercentText(event.target.value)}
              className="w-28"
            />
            <Button
              variant="secondary"
              size="lg"
              loading={busy === "rollout"}
              disabled={!percentChanged || busy !== null}
              onClick={() => {
                if (percent === null) return;
                if (current.enabled && percent > current.rolloutPercent) {
                  setRaiseTo(percent);
                  return;
                }
                void write({ rolloutPercent: percent }, "rollout").catch(
                  (err) =>
                    setError(describeApiError(err, "change the rollout")),
                );
              }}
            >
              Save rollout
            </Button>
          </div>
        </FieldPlate>
        <p className="text-ui-xs text-pretty text-ink-3">
          Each install has a fixed bucket from 0 to 99 and hands off once its
          bucket is below the rollout, so raising it only adds installs. An app
          that cannot reach this switch stays put.
        </p>
        {error ? (
          <p role="alert" className="text-ui-sm text-danger-ink">
            {error}
          </p>
        ) : null}
        {data.updated.nativeHandoff ? (
          <p
            className="text-ui-xs text-ink-3"
            title={formatDateTime(data.updated.nativeHandoff.at)}
          >
            Last changed {formatAgo(data.updated.nativeHandoff.at, now)}.
          </p>
        ) : null}
      </div>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Turn on the native handoff?"
        description={
          current.rolloutPercent > 0
            ? `${current.rolloutPercent}% of Tauri installs will install the native app and close the next time they check for updates, once the latest release is a native one.`
            : "The rollout is 0%, so no install moves until you raise it."
        }
        confirmLabel="Turn on"
        errorAction="turn the handoff on"
        onConfirm={() => write({ enabled: true }, "switch")}
      />
      <ConfirmDialog
        open={raiseTo !== null}
        onOpenChange={(open) => {
          if (!open) setRaiseTo(null);
        }}
        title={raiseTo !== null ? `Raise the rollout to ${raiseTo}%?` : ""}
        description={
          raiseTo !== null
            ? `The handoff is on: ${raiseTo}% of Tauri installs (now ${current.rolloutPercent}%) will install the native app and close the next time they check for updates.`
            : null
        }
        confirmLabel="Raise rollout"
        errorAction="change the rollout"
        onConfirm={() =>
          raiseTo !== null
            ? write({ rolloutPercent: raiseTo }, "rollout")
            : undefined
        }
      />
    </RuleCard>
  );
}

const EXPIRY_CHOICES = [
  { value: "none", label: "No end" },
  { value: "1", label: "1 h" },
  { value: "24", label: "1 day" },
  { value: "168", label: "1 week" },
] as const;
type ExpiryChoice = (typeof EXPIRY_CHOICES)[number]["value"];

function AnnouncementCard({
  settings,
  now,
}: {
  settings: SettingsResource;
  now: number;
}) {
  const transport = useAdminTransport();
  const data = settings.data!;
  const current = data.announcement;
  const live = activeAnnouncement(current, now);
  const [text, setText] = useState(current?.text ?? "");
  const [tone, setTone] = useState<AnnouncementTone>(current?.tone ?? "info");
  const [linkUrl, setLinkUrl] = useState(current?.link_url ?? "");
  const [linkLabel, setLinkLabel] = useState(current?.link_label ?? "");
  const [expiry, setExpiry] = useState<ExpiryChoice>("none");
  const [busy, setBusy] = useState<"publish" | "clear" | null>(null);
  const [error, setError] = useState<string | null>(null);

  // A save elsewhere (or the first load) resets the form to what is stored.
  useEffect(() => {
    setText(current?.text ?? "");
    setTone(current?.tone ?? "info");
    setLinkUrl(current?.link_url ?? "");
    setLinkLabel(current?.link_label ?? "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.id]);

  const preview: SiteAnnouncement | null = text.trim()
    ? {
        id: "preview",
        text: text.trim(),
        tone,
        link_url: linkUrl.trim() || null,
        link_label: linkLabel.trim() || null,
        expires_at: null,
        updated_at: new Date(now).toISOString(),
      }
    : null;

  const publish = async (clear: boolean) => {
    setBusy(clear ? "clear" : "publish");
    setError(null);
    try {
      const next = await transport.send<AdminSettingsResponse>(
        "PUT",
        "/api/settings",
        {
          announcement: clear
            ? null
            : {
                text,
                tone,
                link_url: linkUrl.trim() || null,
                link_label: linkLabel.trim() || null,
                expires_at:
                  expiry === "none"
                    ? null
                    : new Date(
                        Date.now() + Number(expiry) * 3_600_000,
                      ).toISOString(),
              },
        },
      );
      settings.mutate(() => next);
      toast.success(clear ? "Banner cleared" : "Banner published");
    } catch (err) {
      setError(
        describeApiError(
          err,
          clear ? "clear the banner" : "publish the banner",
        ),
      );
    } finally {
      setBusy(null);
    }
  };

  const remaining = ANNOUNCEMENT_MAX_LENGTH - text.length;

  return (
    <RuleCard
      legend="Announcement"
      index={
        live ? (
          <LedLabel state="talk">Showing</LedLabel>
        ) : current ? (
          <LedLabel state="off">Ended</LedLabel>
        ) : (
          <LedLabel state="off">None</LedLabel>
        )
      }
      title="Site banner"
    >
      <div className="flex flex-col gap-5">
        <p className="text-ui-sm text-pretty text-ink-2">
          A strip across the top of Yap on web and the desktop app. A critical
          banner cannot be dismissed.
        </p>
        <FieldPlate
          label="Message"
          htmlFor="admin-announcement-text"
          hint={
            <Num className={remaining < 20 ? "text-warning-ink" : undefined}>
              {remaining}
            </Num>
          }
        >
          <Textarea
            id="admin-announcement-text"
            value={text}
            maxLength={ANNOUNCEMENT_MAX_LENGTH}
            placeholder="Voice is being moved to a new server tonight at 22:00 UTC."
            onChange={(event) => setText(event.target.value)}
            className="min-h-20"
          />
        </FieldPlate>
        <div className="flex flex-col gap-1.5">
          <p className="silkscreen text-ink-3">Tone</p>
          <SegmentedControl
            label="Tone"
            size="sm"
            items={[
              { value: "info", label: "Info" },
              { value: "warning", label: "Warning" },
              { value: "critical", label: "Critical" },
            ]}
            value={tone}
            onValueChange={setTone}
          />
        </div>
        <div className="grid gap-4 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
          <FieldPlate
            label="Link"
            htmlFor="admin-announcement-link"
            hint="Optional"
          >
            <Input
              id="admin-announcement-link"
              type="url"
              inputMode="url"
              value={linkUrl}
              placeholder="https://"
              onChange={(event) => setLinkUrl(event.target.value)}
            />
          </FieldPlate>
          <FieldPlate label="Link text" htmlFor="admin-announcement-link-label">
            <Input
              id="admin-announcement-link-label"
              value={linkLabel}
              maxLength={40}
              placeholder="Read more"
              disabled={!linkUrl.trim()}
              onChange={(event) => setLinkLabel(event.target.value)}
            />
          </FieldPlate>
        </div>
        <div className="flex flex-col gap-1.5">
          <p className="silkscreen text-ink-3">Ends</p>
          <SegmentedControl
            label="Ends"
            size="sm"
            items={EXPIRY_CHOICES}
            value={expiry}
            onValueChange={setExpiry}
          />
        </div>
        <div className="flex flex-col gap-2">
          <p className="silkscreen text-ink-3">Preview</p>
          <div className="overflow-hidden rounded-[2px] border border-line-strong">
            {preview ? (
              <SiteAnnouncementBar
                announcement={preview}
                onDismiss={() => {}}
              />
            ) : (
              <p className="px-3 py-2 text-ui-sm text-ink-3">
                Write a message to see it.
              </p>
            )}
          </div>
          {live?.expires_at ? (
            <p className="text-ui-xs text-ink-3">
              The live banner ends {formatUntil(live.expires_at, now)} (
              {formatDateTime(live.expires_at)}).
            </p>
          ) : null}
        </div>
        {error ? (
          <p role="alert" className="text-ui-sm text-danger-ink">
            {error}
          </p>
        ) : null}
        <div className="flex flex-wrap items-center gap-3">
          <Button
            variant="primary"
            size="lg"
            loading={busy === "publish"}
            disabled={!text.trim() || busy !== null}
            onClick={() => void publish(false)}
          >
            {live ? "Update banner" : "Publish banner"}
          </Button>
          {current ? (
            <Button
              variant="secondary"
              size="lg"
              paired
              loading={busy === "clear"}
              disabled={busy !== null}
              onClick={() => void publish(true)}
            >
              Clear
            </Button>
          ) : null}
        </div>
        <p className="text-ui-xs text-pretty text-ink-3">
          The Qt desktop app and the mobile app don&apos;t show it yet: each
          needs to read{" "}
          <code className="font-mono text-[12px] whitespace-nowrap">
            GET /api/site-config
          </code>{" "}
          and draw this strip.
        </p>
      </div>
    </RuleCard>
  );
}
