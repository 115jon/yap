/**
 * The board editors' fields: grapheme counters, the Loud lock and upsell,
 * the image field, and `WidgetInspector`, one widget's settings by type.
 * The inspector is the Studio's right pane and the "More options" popover
 * in edit in place: same fields, same store, same undo stack.
 */

import { useId, useRef, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { IconButton } from "@/components/ui/IconButton";
import { FieldPlate, Input } from "@/components/ui/input";
import { SegmentedControl } from "@/components/ui/segmented-control";
import { getBadge } from "@/lib/badges";
import {
  cleanUrl,
  linkHost,
  MAX_BADGES_SHOWN,
  MAX_CUSTOM_LINES,
  MAX_LINKS,
  MAX_PHOTOS,
  MAX_SERVERS_SHOWN,
  MOOD_POSES,
  TEXT_LIMITS,
  textLength,
  WIDGET_SPECS,
  type AnyProfileWidget,
  type BoardData,
  type DeadlockAtRest,
  type MoodPose,
  type TopGamesRange,
  type VoiceHoursPeriod,
  type WidgetConfigMap,
  type WidgetType,
} from "@/lib/profile-board";
import { canAdd, SIZE_LABELS } from "@/lib/profile-board-editor";
import { cn } from "@/lib/utils";
import { useChatStore } from "@/stores/chat-store";
import { openSettings } from "@/components/chat/settings/settings-links";
import { Switch } from "@/components/ui/switch";
import { DEADLOCK_CREDIT, type DeadlockRange } from "@/lib/deadlock";

import { useBoardMedia } from "./board-context";
import { WIDGET_ICON } from "./board-format";
import {
  useWidgetAssetUpload,
  type BoardEditorStore,
  type WidgetAssetUpload,
} from "./useBoardEditor";

// ── Text ───────────────────────────────────────────────────────────────────

let segmenter: Intl.Segmenter | null = null;

/** `value` cut to `max` grapheme clusters (what the counters count). */
export function clampText(value: string, max: number): string {
  if (value.length <= max) return value;
  if (typeof Intl !== "undefined" && "Segmenter" in Intl) {
    segmenter ??= new Intl.Segmenter(undefined, { granularity: "grapheme" });
    const parts = Array.from(segmenter.segment(value), (part) => part.segment);
    return parts.length > max ? parts.slice(0, max).join("") : value;
  }
  const parts = Array.from(value);
  return parts.length > max ? parts.slice(0, max).join("") : value;
}

/** "11/24": always in the inspector, near the limit on the board. */
export function CharCount({
  value,
  max,
  near = false,
  className,
}: {
  value: string;
  max: number;
  /** Show only within 10 of the limit. */
  near?: boolean;
  className?: string;
}) {
  const used = textLength(value);
  if (near && used < max - 10) return null;
  return (
    <span
      data-slot="char-count"
      aria-label={`${used} of ${max} characters`}
      className={cn(
        "text-num-xs tabular-nums",
        used >= max ? "text-warning-ink" : "text-ink-3",
        className,
      )}
    >
      {used}/{max}
    </span>
  );
}

// ── Plan marks ─────────────────────────────────────────────────────────────

/** The accent-ink "Loud" mark on a paid widget the plan doesn't cover. */
export function LoudLock({ className }: { className?: string }) {
  return (
    <span
      data-slot="loud-lock"
      className={cn(
        "silkscreen inline-flex h-5 shrink-0 items-center gap-1 rounded-[2px] bg-accent-tint px-1.5 text-accent-ink",
        className,
      )}
    >
      <Icon name="lock" size="xs" />
      Loud
    </span>
  );
}

/** The Custom widget on a free plan: one plate, one sentence, no button. */
export function LoudUpsell() {
  return (
    <div
      data-slot="loud-upsell"
      className="flex flex-col gap-2 rounded-[2px] border border-line-strong bg-ply-2 p-4"
    >
      <LoudLock className="self-start" />
      <p className="font-display text-[17px] leading-[22px] font-bold text-balance text-ink-1">
        Custom widgets come with Yap Loud
      </p>
      <p className="text-ui-sm text-pretty text-ink-2">
        Lay one out and fill it in now. Only you see it until you're on Loud or
        Founder, then it goes live as it is.
      </p>
    </div>
  );
}

// ── Fields ─────────────────────────────────────────────────────────────────

type Ground = "ply-0" | "ply-1" | "ply-2" | "ply-3";

/** A legend-plate text field with a grapheme counter on the legend line. */
export function TextField({
  label,
  value,
  max,
  onChange,
  placeholder,
  ground,
  invalid,
  describedBy,
}: {
  label: string;
  value: string;
  max: number;
  onChange(value: string): void;
  placeholder?: string;
  ground: Ground;
  invalid?: boolean;
  describedBy?: string;
}) {
  const id = useId();
  return (
    <FieldPlate
      label={label}
      htmlFor={id}
      ground={ground}
      hint={<CharCount value={value} max={max} />}
    >
      <Input
        id={id}
        value={value}
        placeholder={placeholder}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        onChange={(event) => onChange(clampText(event.target.value, max))}
      />
    </FieldPlate>
  );
}

/** A small silkscreen section rule inside the inspector. */
function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3 border-t-2 border-ink-1 pt-2">
      <div className="flex min-h-5 items-center justify-between gap-2">
        <h4 className="silkscreen text-ink-2">{title}</h4>
        {aside}
      </div>
      {children}
    </section>
  );
}

/** One uploaded picture with its replace, remove and limit text. */
export function ImageField({
  label,
  asset,
  ownerId,
  upload,
  onChange,
  disabled,
}: {
  label: string;
  asset: string | null;
  ownerId: string;
  upload: WidgetAssetUpload;
  onChange(asset: string | null): void;
  disabled?: boolean;
}) {
  const media = useBoardMedia();
  const inputRef = useRef<HTMLInputElement | null>(null);
  const errorId = useId();
  return (
    <div className="flex flex-col gap-2">
      <div className="flex min-w-0 items-center gap-3 rounded-[2px] border border-line-field bg-ply-2 p-2.5">
        <span className="relative size-14 shrink-0 overflow-hidden rounded-[2px] bg-ply-4">
          {asset ? (
            <img
              src={media.asset(ownerId, asset)}
              alt=""
              className="absolute inset-0 size-full object-cover"
            />
          ) : (
            <Icon
              name="image"
              size="lg"
              className="absolute inset-0 m-auto text-ink-3"
            />
          )}
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <p className="truncate text-ui-sm font-medium text-ink-1">
            {asset ? label : "No picture yet"}
          </p>
          <p className="text-ui-xs text-pretty text-ink-3">
            PNG, JPEG, WebP or GIF, up to {upload.limitText}.{" "}
            {upload.animated
              ? "Animated GIFs play on hover."
              : "Animated images need Loud."}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {asset ? (
            <IconButton
              icon="close"
              size={32}
              variant="muted"
              aria-label={`Remove ${label.toLowerCase()}`}
              disabled={disabled}
              onClick={() => onChange(null)}
            />
          ) : null}
          <IconButton
            icon="upload"
            size={32}
            aria-label={asset ? "Replace picture" : "Upload a picture"}
            disabled={disabled || upload.busy}
            aria-describedby={upload.error ? errorId : undefined}
            onClick={() => inputRef.current?.click()}
          />
        </div>
        <input
          ref={inputRef}
          type="file"
          accept="image/png,image/jpeg,image/webp,image/gif"
          className="sr-only"
          tabIndex={-1}
          aria-hidden="true"
          onChange={async (event) => {
            const file = event.target.files?.[0];
            event.target.value = "";
            if (!file) return;
            const name = await upload.upload(file);
            if (name) onChange(name);
          }}
        />
      </div>
      {upload.busy ? (
        <p role="status" className="text-ui-xs text-ink-3">
          Uploading…
        </p>
      ) : null}
      {upload.error ? (
        <p id={errorId} role="alert" className="text-ui-xs text-danger-ink">
          {upload.error}
        </p>
      ) : null}
    </div>
  );
}

// ── Inspector ──────────────────────────────────────────────────────────────

export interface WidgetInspectorProps {
  editor: BoardEditorStore;
  widget: AnyProfileWidget;
  data: BoardData;
  /** "panel": the Studio's right pane (size, remove, duplicate). */
  variant?: "panel" | "popover";
  ground?: Ground;
  /** After Remove (focus moves elsewhere). */
  onRemoved?(): void;
}

const MOOD_LABELS: Record<MoodPose, string> = {
  bark: "Bark",
  stuffed: "Stuffed",
  sad: "Sad",
  squashed: "Squashed",
};

export function WidgetInspector({
  editor,
  widget,
  data,
  variant = "panel",
  ground = "ply-3",
  onRemoved,
}: WidgetInspectorProps) {
  const spec = WIDGET_SPECS[widget.type];
  const upload = useWidgetAssetUpload(editor.entitlement);
  const locked = !!spec.perk && !editor.canCustom;
  const set = <T extends WidgetType>(
    cfg: Partial<WidgetConfigMap[T]>,
    coalesce?: string,
  ) =>
    editor.dispatch({
      type: "updateCfg",
      id: widget.id,
      cfg: cfg as Partial<WidgetConfigMap[WidgetType]>,
      coalesce,
    });
  const ownerId = editor.response.user_id;

  const fields = (
    <TypeFields
      widget={widget}
      data={data}
      set={set}
      ground={ground}
      upload={upload}
      ownerId={ownerId}
    />
  );

  return (
    <div
      data-slot="widget-inspector"
      data-type={widget.type}
      className="flex flex-col gap-5"
    >
      {variant === "panel" ? (
        <header className="flex items-center gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-[2px] border border-line-strong text-ink-2">
            <Icon name={WIDGET_ICON[widget.type]} size="md" />
          </span>
          <div className="min-w-0">
            <h3 className="truncate font-display text-[20px] leading-6 font-bold text-ink-1">
              {spec.name}
            </h3>
            <p className="text-ui-xs text-ink-3">
              {locked ? "Draft · only you" : spec.blurb.replace(/\.$/, "")}
            </p>
          </div>
        </header>
      ) : null}

      {locked ? <LoudUpsell /> : null}

      <Section
        title={locked ? "Draft content" : "Content"}
        aside={spec.perk && !locked ? <LoudLock /> : undefined}
      >
        {fields}
      </Section>

      {variant === "panel" ? (
        <>
          <Section
            title="Size"
            aside={
              <span className="text-num-xs text-ink-3">{widget.size}</span>
            }
          >
            <SegmentedControl
              label={`${spec.name} size`}
              items={spec.sizes.map((size) => ({
                value: size,
                label: SIZE_LABELS[size],
              }))}
              value={widget.size}
              onValueChange={(size) =>
                editor.dispatch({ type: "resize", id: widget.id, size })
              }
              className="w-full"
            />
          </Section>
          <div className="flex flex-wrap gap-2 border-t border-line pt-4">
            <Button
              variant="destructive"
              size="sm"
              onClick={() => {
                editor.dispatch({ type: "remove", id: widget.id });
                onRemoved?.();
              }}
            >
              <Icon name="trash" size="sm" />
              Remove
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={!canAdd(editor.state.draft, widget.type)}
              onClick={() =>
                editor.dispatch({ type: "duplicate", id: widget.id })
              }
            >
              <Icon name="copy" size="sm" />
              Duplicate
            </Button>
          </div>
        </>
      ) : null}
    </div>
  );
}

function TypeFields({
  widget,
  data,
  set,
  ground,
  upload,
  ownerId,
}: {
  widget: AnyProfileWidget;
  data: BoardData;
  set: <T extends WidgetType>(
    cfg: Partial<WidgetConfigMap[T]>,
    coalesce?: string,
  ) => void;
  ground: Ground;
  upload: WidgetAssetUpload;
  ownerId: string;
}) {
  switch (widget.type) {
    case "now_playing":
      return (
        <ChoiceField
          label="When you're not playing"
          value={widget.cfg.when_idle}
          items={[
            { value: "last_played", label: "Last played" },
            { value: "hide", label: "Hide it" },
          ]}
          onChange={(when_idle) => set<"now_playing">({ when_idle })}
        />
      );
    case "top_games":
      return (
        <ChoiceField
          label="Range"
          value={widget.cfg.range}
          items={[
            { value: "7d" as TopGamesRange, label: "7 days" },
            { value: "30d" as TopGamesRange, label: "30 days" },
            { value: "all" as TopGamesRange, label: "All time" },
          ]}
          onChange={(range) => set<"top_games">({ range })}
        />
      );
    case "deadlock":
      return (
        <DeadlockFields
          cfg={widget.cfg}
          data={data}
          onChange={(cfg) => set<"deadlock">(cfg)}
        />
      );
    case "voice_hours":
      return (
        <ChoiceField
          label="Period"
          value={widget.cfg.period}
          items={[
            { value: "month" as VoiceHoursPeriod, label: "This month" },
            { value: "30d" as VoiceHoursPeriod, label: "Last 30 days" },
          ]}
          onChange={(period) => set<"voice_hours">({ period })}
        />
      );
    case "listening":
      return (
        <Note>
          Shows the track you share in Listen Together, or the last one. It
          hides when you haven't listened yet.
        </Note>
      );
    case "guestbook":
      return (
        <Note>
          People you share a server or a DM with leave short notes. You can
          close your guestbook in profile settings.
        </Note>
      );
    case "badges":
      return (
        <BadgePicker
          picked={widget.cfg.badge_ids}
          earned={data.badges ?? []}
          onChange={(badge_ids) => set<"badges">({ badge_ids })}
        />
      );
    case "servers":
      return (
        <ServerPicker
          picked={widget.cfg.server_ids}
          known={data.servers ?? []}
          onChange={(server_ids) => set<"servers">({ server_ids })}
        />
      );
    case "quote":
      return (
        <QuoteField
          messageId={widget.cfg.message_id}
          snapshotText={widget.cfg.snapshot?.text ?? null}
          ground={ground}
          onChange={(message_id) =>
            set<"quote">({ message_id, snapshot: null })
          }
        />
      );
    case "mood":
      return (
        <>
          <ChoiceField
            label="Pose"
            value={widget.cfg.pose}
            items={MOOD_POSES.map((pose) => ({
              value: pose,
              label: MOOD_LABELS[pose],
            }))}
            onChange={(pose) => set<"mood">({ pose })}
          />
          <TextField
            label="Mood"
            value={widget.cfg.text}
            max={TEXT_LIMITS.mood}
            ground={ground}
            placeholder="ranked grind. send snacks"
            onChange={(text) => set<"mood">({ text }, "text")}
          />
          <ClearsField
            value={widget.cfg.clears_at}
            onChange={(clears_at) => set<"mood">({ clears_at })}
          />
        </>
      );
    case "countdown":
      return (
        <>
          <TextField
            label="Title"
            value={widget.cfg.title}
            max={TEXT_LIMITS.countdownTitle}
            ground={ground}
            placeholder="Raid night"
            onChange={(title) => set<"countdown">({ title }, "title")}
          />
          <DateTimeField
            label="Counting down to"
            value={widget.cfg.at}
            ground={ground}
            onChange={(at) => set<"countdown">({ at })}
          />
        </>
      );
    case "links":
      return (
        <LinksField
          links={widget.cfg.links}
          ground={ground}
          onChange={(links, coalesce) => set<"links">({ links }, coalesce)}
        />
      );
    case "photos":
      return (
        <>
          <PhotosField
            photos={widget.cfg.photos}
            ownerId={ownerId}
            upload={upload}
            ground={ground}
            onChange={(photos, coalesce) => set<"photos">({ photos }, coalesce)}
          />
          <TextField
            label="Caption"
            value={widget.cfg.caption}
            max={TEXT_LIMITS.caption}
            ground={ground}
            onChange={(caption) => set<"photos">({ caption }, "caption")}
          />
        </>
      );
    case "custom": {
      const lines = Array.from(
        { length: MAX_CUSTOM_LINES },
        (_, index) => widget.cfg.lines[index] ?? "",
      );
      return (
        <>
          <TextField
            label="Label"
            value={widget.cfg.legend}
            max={TEXT_LIMITS.legend}
            ground={ground}
            placeholder="Custom"
            onChange={(legend) => set<"custom">({ legend }, "legend")}
          />
          <TextField
            label="Title"
            value={widget.cfg.title}
            max={TEXT_LIMITS.title}
            ground={ground}
            onChange={(title) => set<"custom">({ title }, "title")}
          />
          <ImageField
            label="Picture"
            asset={widget.cfg.asset}
            ownerId={ownerId}
            upload={upload}
            onChange={(asset) => set<"custom">({ asset })}
          />
          {lines.map((line, index) => (
            <TextField
              key={index}
              label={`Line ${index + 1}`}
              value={line}
              max={TEXT_LIMITS.line}
              ground={ground}
              onChange={(value) => {
                const next = [...lines];
                next[index] = value;
                set<"custom">({ lines: trimLines(next) }, `line-${index}`);
              }}
            />
          ))}
        </>
      );
    }
  }
}

/** Drops empty trailing lines so the stored list stays short. */
export function trimLines(lines: string[]): string[] {
  const next = [...lines];
  while (next.length > 0 && next.at(-1) === "") next.pop();
  return next;
}

/**
 * The Deadlock plate's setup (mockup 05, in the inspector): the Steam
 * link, what the plate shows at rest, the range, the party, the
 * `-condebug` launch option for live matches, and where the data comes
 * from. Every game mode counts; there is no ranked-only switch.
 */
function DeadlockFields({
  cfg,
  data,
  onChange,
}: {
  cfg: WidgetConfigMap["deadlock"];
  data: BoardData;
  onChange(cfg: Partial<WidgetConfigMap["deadlock"]>): void;
}) {
  const partyId = useId();
  const [copied, setCopied] = useState(false);
  const deadlock = data.deadlock;
  const linked = deadlock && deadlock.state !== "not_linked";
  return (
    <div data-slot="deadlock-fields" className="flex flex-col gap-4">
      {linked ? (
        <div className="flex items-center gap-2.5 rounded-[2px] bg-ply-2 p-2.5 shadow-[inset_0_0_0_1px_var(--line-strong)]">
          <Icon name="steam" size="md" className="shrink-0 text-ink-2" />
          <div className="min-w-0 flex-1">
            <p className="truncate text-ui-sm font-semibold text-ink-1">
              {deadlock.persona ?? "Steam account"}
            </p>
            <p className="flex items-center gap-1 text-ui-xs text-success-ink">
              <Icon name="verified" size="xs" />
              Verified through Steam
            </p>
          </div>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => openSettings({ panel: "connections" })}
          >
            Manage
          </Button>
        </div>
      ) : (
        <div className="flex flex-col items-start gap-2">
          <Note>
            Deadlock stats belong to a Steam account. Link the one you play on
            in Connections; Steam tells Yap your account ID and nothing else.
          </Note>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => openSettings({ panel: "connections" })}
          >
            <Icon name="steam" size="sm" />
            Link Steam
          </Button>
        </div>
      )}
      <ChoiceField
        label="When you're not playing"
        value={cfg.at_rest}
        items={[
          { value: "career" as DeadlockAtRest, label: "Career" },
          { value: "last_match" as DeadlockAtRest, label: "Last match" },
        ]}
        onChange={(at_rest) => onChange({ at_rest })}
      />
      <ChoiceField
        label="Count"
        value={cfg.range}
        items={[
          { value: "7d" as DeadlockRange, label: "7 days" },
          { value: "30d" as DeadlockRange, label: "30 days" },
          { value: "season" as DeadlockRange, label: "Season" },
        ]}
        onChange={(range) => onChange({ range })}
      />
      <p className="-mt-2 text-ui-xs text-pretty text-ink-3">
        Every mode counts: ranked, unranked and Street Brawl.
      </p>
      <div className="flex items-center justify-between gap-3">
        <label htmlFor={partyId} className="flex min-w-0 flex-col">
          <span className="text-ui-sm font-medium text-ink-1">Show party</span>
          <span className="text-ui-xs text-pretty text-ink-3">
            Names friends in your party who linked Steam on Yap too.
          </span>
        </label>
        <Switch
          id={partyId}
          checked={cfg.show_party}
          onCheckedChange={(show_party) => onChange({ show_party })}
        />
      </div>
      <div className="flex flex-col gap-1.5 border-t border-line pt-4">
        <span className="silkscreen text-ink-3">Live match on desktop</span>
        <p className="text-ui-xs text-pretty text-ink-2">
          Deadlock writes what you're doing to a log file when it starts with{" "}
          <code className="rounded-[2px] bg-tint-2 px-1.5 py-0.5 font-silk text-[12px] text-ink-1">
            -condebug
          </code>
          . Yap desktop reads that file and nothing else: no memory reads,
          nothing added to the game.
        </p>
        <div className="flex h-9 items-center gap-2 rounded-[2px] bg-ply-0 pr-1 pl-2.5 shadow-[inset_0_0_0_1px_var(--line-strong)]">
          <code className="font-silk text-[12px] font-semibold text-ink-1">
            -condebug
          </code>
          <IconButton
            className="ml-auto"
            size={28}
            tooltip={copied ? "Copied" : "Copy launch option"}
            icon={copied ? "check" : "copy"}
            onClick={() => {
              void navigator.clipboard?.writeText("-condebug").then(
                () => setCopied(true),
                () => undefined,
              );
            }}
          />
        </div>
        <p className="text-ui-xs text-pretty text-ink-3">
          Steam › Deadlock › Properties › Launch options. Without it you still
          get career stats and “Playing Deadlock”.
        </p>
      </div>
      <div className="flex flex-col gap-1.5 rounded-[2px] bg-ply-2 p-3 shadow-[inset_0_0_0_1px_var(--line-strong)]">
        <p className="flex items-center gap-2 font-display text-[14px] leading-[18px] font-bold text-ink-1">
          <Icon name="globe" size="sm" />
          Your matches are public anyway
        </p>
        <p className="text-ui-xs text-pretty text-ink-2">
          Deadlock match history is public on deadlock-api.com and tracker sites
          whether or not you link it. Linking only changes what Yap shows. To
          remove it at the source, use{" "}
          <a
            href="https://deadlock-api.com/data-privacy"
            target="_blank"
            rel="noreferrer noopener"
            className="text-ink-1 underline underline-offset-[3px]"
          >
            deadlock-api.com/data-privacy
          </a>
          .
        </p>
        <p className="text-ui-xs text-ink-3">{DEADLOCK_CREDIT}</p>
      </div>
    </div>
  );
}

function Note({ children }: { children: ReactNode }) {
  return <p className="text-ui-sm text-pretty text-ink-2">{children}</p>;
}

function ChoiceField<T extends string>({
  label,
  value,
  items,
  onChange,
}: {
  label: string;
  value: T;
  items: { value: T; label: string }[];
  onChange(value: T): void;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-ui-xs font-medium text-ink-2">{label}</span>
      <SegmentedControl
        label={label}
        size="sm"
        items={items}
        value={value}
        onValueChange={onChange}
        className="w-full"
      />
    </div>
  );
}

function toLocalInput(iso: string | null): string {
  if (!iso) return "";
  const at = Date.parse(iso);
  if (!Number.isFinite(at)) return "";
  const date = new Date(at);
  const local = new Date(at - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 16);
}

function fromLocalInput(value: string): string | null {
  if (!value) return null;
  const at = new Date(value).getTime();
  return Number.isFinite(at) ? new Date(at).toISOString() : null;
}

export function DateTimeField({
  label,
  value,
  ground,
  onChange,
}: {
  label: string;
  value: string | null;
  ground: Ground;
  onChange(value: string | null): void;
}) {
  const id = useId();
  return (
    <FieldPlate label={label} htmlFor={id} ground={ground}>
      <Input
        id={id}
        type="datetime-local"
        value={toLocalInput(value)}
        className="font-silk tabular-nums [color-scheme:dark] [:root[data-theme*=light]_&]:[color-scheme:light]"
        onChange={(event) => onChange(fromLocalInput(event.target.value))}
      />
    </FieldPlate>
  );
}

type ClearsChoice = "never" | "hour" | "midnight" | "day";

/** When a mood picked as `choice` clears, from now. */
function clearsAt(choice: ClearsChoice): string | null {
  const now = Date.now();
  if (choice === "never") return null;
  if (choice === "hour") return new Date(now + 3_600_000).toISOString();
  if (choice === "day") return new Date(now + 86_400_000).toISOString();
  const midnight = new Date(now);
  midnight.setHours(24, 0, 0, 0);
  return midnight.toISOString();
}

function ClearsField({
  value,
  onChange,
}: {
  value: string | null;
  onChange(value: string | null): void;
}) {
  const pick = (next: ClearsChoice) => onChange(clearsAt(next));
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-ui-xs font-medium text-ink-2">
        Clears{" "}
        {value ? (
          <span className="text-ink-3">· {formatClears(value)}</span>
        ) : null}
      </span>
      <div
        className="flex flex-wrap gap-1.5"
        role="group"
        aria-label="Clear mood"
      >
        {(
          [
            ["never", "Never"],
            ["hour", "In an hour"],
            ["midnight", "At midnight"],
            ["day", "Tomorrow"],
          ] as const
        ).map(([key, label]) => (
          <Button
            key={key}
            variant="outline"
            size="xs"
            onClick={() => pick(key)}
          >
            {label}
          </Button>
        ))}
      </div>
    </div>
  );
}

const CLEARS = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  hour: "2-digit",
  minute: "2-digit",
});

function formatClears(iso: string): string {
  const at = Date.parse(iso);
  return Number.isFinite(at) ? CLEARS.format(at) : "";
}

function QuoteField({
  messageId,
  snapshotText,
  ground,
  onChange,
}: {
  messageId: string | null;
  snapshotText: string | null;
  ground: Ground;
  onChange(messageId: string | null): void;
}) {
  const id = useId();
  const hintId = useId();
  // What was typed (a link or an id); follows the widget when undo, redo or
  // a load changes its message under the field.
  const [raw, setRaw] = useState(messageId ?? "");
  if (messageIdFrom(raw) !== messageId) setRaw(messageId ?? "");
  return (
    <div className="flex flex-col gap-2">
      <FieldPlate label="Message link or ID" htmlFor={id} ground={ground}>
        <Input
          id={id}
          value={raw}
          placeholder="Paste a message link"
          aria-describedby={hintId}
          onChange={(event) => {
            setRaw(event.target.value);
            onChange(messageIdFrom(event.target.value));
          }}
        />
      </FieldPlate>
      <p id={hintId} className="text-ui-xs text-pretty text-ink-3">
        A message you sent or were mentioned in. Copy its link from the message
        menu; it's pinned as it reads when you save.
      </p>
      {snapshotText ? (
        <blockquote className="border-l border-line-strong pl-3 text-ui-sm text-pretty text-ink-2">
          {snapshotText}
        </blockquote>
      ) : null}
    </div>
  );
}

/** The message id in a pasted link (its last path segment) or a bare id. */
export function messageIdFrom(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const last = trimmed.split(/[/?#]/).filter(Boolean).at(-1) ?? "";
  const candidate = /^[A-Za-z0-9_-]{1,64}$/.test(trimmed) ? trimmed : last;
  return /^[A-Za-z0-9_-]{1,64}$/.test(candidate) ? candidate : null;
}

function LinksField({
  links,
  ground,
  onChange,
}: {
  links: WidgetConfigMap["links"]["links"];
  ground: Ground;
  onChange(links: WidgetConfigMap["links"]["links"], coalesce?: string): void;
}) {
  const update = (index: number, patch: Partial<(typeof links)[number]>) => {
    const next = links.map((link, i) =>
      i === index ? { ...link, ...patch } : link,
    );
    onChange(next, `link-${index}-${Object.keys(patch)[0]}`);
  };
  return (
    <div className="flex flex-col gap-4">
      {links.map((link, index) => (
        <LinkRow
          key={index}
          index={index}
          label={link.label}
          url={link.url}
          ground={ground}
          onLabel={(label) => update(index, { label })}
          onUrl={(url) => update(index, { url })}
          onRemove={() => onChange(links.filter((_, i) => i !== index))}
        />
      ))}
      {links.length < MAX_LINKS ? (
        <Button
          variant="outline"
          size="sm"
          className="self-start"
          onClick={() => onChange([...links, { label: "", url: "" }])}
        >
          <Icon name="add" size="sm" />
          Add a link
        </Button>
      ) : (
        <p className="text-ui-xs text-ink-3">That's {MAX_LINKS}, the most.</p>
      )}
    </div>
  );
}

function LinkRow({
  index,
  label,
  url,
  ground,
  onLabel,
  onUrl,
  onRemove,
}: {
  index: number;
  label: string;
  url: string;
  ground: Ground;
  onLabel(value: string): void;
  onUrl(value: string): void;
  onRemove(): void;
}) {
  const urlId = useId();
  const errorId = useId();
  const clean = url ? cleanUrl(url) : null;
  const invalid = !!url && !clean;
  return (
    <div className="flex flex-col gap-2 rounded-[2px] border border-line p-2.5">
      <div className="flex items-center justify-between gap-2">
        <span className="silkscreen text-ink-3">Link {index + 1}</span>
        <IconButton
          icon="close"
          size={24}
          variant="muted"
          aria-label={`Remove link ${index + 1}`}
          onClick={onRemove}
        />
      </div>
      <TextField
        label="Label"
        value={label}
        max={TEXT_LIMITS.linkLabel}
        ground={ground}
        placeholder={clean ? linkHost(clean) : "Art blog"}
        onChange={onLabel}
      />
      <FieldPlate label="Address" htmlFor={urlId} ground={ground}>
        <Input
          id={urlId}
          value={url}
          inputMode="url"
          placeholder="kiwi.page/art"
          aria-invalid={invalid || undefined}
          aria-describedby={invalid ? errorId : undefined}
          onChange={(event) =>
            onUrl(event.target.value.slice(0, TEXT_LIMITS.url))
          }
        />
      </FieldPlate>
      {invalid ? (
        <p id={errorId} className="text-ui-xs text-danger-ink">
          Use an https address, like kiwi.page/art.
        </p>
      ) : clean ? (
        <p className="truncate text-num-xs text-ink-3">{linkHost(clean)}</p>
      ) : null}
    </div>
  );
}

function PhotosField({
  photos,
  ownerId,
  upload,
  ground,
  onChange,
}: {
  photos: WidgetConfigMap["photos"]["photos"];
  ownerId: string;
  upload: WidgetAssetUpload;
  ground: Ground;
  onChange(
    photos: WidgetConfigMap["photos"]["photos"],
    coalesce?: string,
  ): void;
}) {
  return (
    <div className="flex flex-col gap-3">
      {photos.map((photo, index) => (
        <PhotoRow
          key={photo.asset}
          index={index}
          asset={photo.asset}
          alt={photo.alt}
          ownerId={ownerId}
          ground={ground}
          onAlt={(alt) =>
            onChange(
              photos.map((item, i) => (i === index ? { ...item, alt } : item)),
              `alt-${index}`,
            )
          }
          onRemove={() => onChange(photos.filter((_, i) => i !== index))}
        />
      ))}
      {photos.length < MAX_PHOTOS ? (
        <ImageField
          label="New photo"
          asset={null}
          ownerId={ownerId}
          upload={upload}
          onChange={(asset) => {
            if (asset) onChange([...photos, { asset, alt: "" }]);
          }}
        />
      ) : (
        <p className="text-ui-xs text-ink-3">
          That's {MAX_PHOTOS} photos, the most.
        </p>
      )}
    </div>
  );
}

function PhotoRow({
  index,
  asset,
  alt,
  ownerId,
  ground,
  onAlt,
  onRemove,
}: {
  index: number;
  asset: string;
  alt: string;
  ownerId: string;
  ground: Ground;
  onAlt(value: string): void;
  onRemove(): void;
}) {
  const media = useBoardMedia();
  return (
    <div className="flex items-end gap-2.5">
      <span className="relative size-12 shrink-0 overflow-hidden rounded-[2px] bg-ply-4">
        <img
          src={media.asset(ownerId, asset)}
          alt=""
          className="absolute inset-0 size-full object-cover"
        />
      </span>
      <div className="min-w-0 flex-1">
        <TextField
          label={`Photo ${index + 1} description`}
          value={alt}
          max={TEXT_LIMITS.alt}
          ground={ground}
          placeholder="What's in it"
          onChange={onAlt}
        />
      </div>
      <IconButton
        icon="trash"
        size={40}
        variant="destructive"
        aria-label={`Remove photo ${index + 1}`}
        onClick={onRemove}
      />
    </div>
  );
}

function ToggleRow({
  pressed,
  disabled,
  onClick,
  children,
  trailing,
}: {
  pressed: boolean;
  disabled?: boolean;
  onClick(): void;
  children: ReactNode;
  trailing?: ReactNode;
}) {
  return (
    <li className="flex min-w-0 items-center gap-1">
      <button
        type="button"
        aria-pressed={pressed}
        disabled={disabled}
        onClick={onClick}
        className="flex h-9 min-w-0 flex-1 items-center gap-2.5 rounded-[2px] px-2 text-left text-ui-row font-medium text-ink-2 outline-none aria-pressed:text-ink-1 disabled:opacity-45 pointer-fine:hover:enabled:bg-tint-1 focus-visible:ring-2 focus-visible:ring-accent-ink"
      >
        <span
          aria-hidden="true"
          className={cn(
            "grid size-4 shrink-0 place-items-center rounded-[2px] border-2",
            pressed
              ? "border-primary-fill bg-primary-fill text-on-primary"
              : "border-ink-3",
          )}
        >
          {pressed ? <Icon name="check" size="xs" state="active" /> : null}
        </span>
        <span className="min-w-0 truncate">{children}</span>
      </button>
      {trailing}
    </li>
  );
}

function BadgePicker({
  picked,
  earned,
  onChange,
}: {
  picked: string[];
  earned: NonNullable<BoardData["badges"]>;
  onChange(ids: string[]): void;
}) {
  const known = earned.filter((badge) => getBadge(badge.id));
  if (known.length === 0) {
    return <Note>You haven't earned a badge yet. They show up here.</Note>;
  }
  const full = picked.length >= MAX_BADGES_SHOWN;
  const ordered = [
    ...picked.filter((id) => known.some((badge) => badge.id === id)),
    ...known.map((badge) => badge.id).filter((id) => !picked.includes(id)),
  ];
  const move = (id: string, delta: number) => {
    const at = picked.indexOf(id);
    const to = at + delta;
    if (at < 0 || to < 0 || to >= picked.length) return;
    const next = [...picked];
    next.splice(at, 1);
    next.splice(to, 0, id);
    onChange(next);
  };
  return (
    <div className="flex flex-col gap-2">
      <p className="text-ui-xs text-pretty text-ink-3">
        {picked.length === 0
          ? "Showing every badge, newest first. Pick up to 8 to choose and order them."
          : `${picked.length} of ${MAX_BADGES_SHOWN} picked, in this order.`}
      </p>
      <ul className="flex flex-col gap-0.5">
        {ordered.map((id) => {
          const on = picked.includes(id);
          const at = picked.indexOf(id);
          return (
            <ToggleRow
              key={id}
              pressed={on}
              disabled={!on && full}
              onClick={() =>
                onChange(
                  on ? picked.filter((item) => item !== id) : [...picked, id],
                )
              }
              trailing={
                on ? (
                  <>
                    <IconButton
                      icon="chevronUp"
                      size={28}
                      variant="muted"
                      aria-label={`Move ${getBadge(id)?.name} earlier`}
                      disabled={at === 0}
                      onClick={() => move(id, -1)}
                    />
                    <IconButton
                      icon="chevronDown"
                      size={28}
                      variant="muted"
                      aria-label={`Move ${getBadge(id)?.name} later`}
                      disabled={at === picked.length - 1}
                      onClick={() => move(id, 1)}
                    />
                  </>
                ) : null
              }
            >
              {getBadge(id)?.name ?? id}
            </ToggleRow>
          );
        })}
      </ul>
    </div>
  );
}

function ServerPicker({
  picked,
  known,
  onChange,
}: {
  picked: string[];
  known: NonNullable<BoardData["servers"]>;
  onChange(ids: string[]): void;
}) {
  const storeServers = useChatStore((state) => state.servers);
  const options = new Map<string, string>();
  for (const server of storeServers) options.set(server.id, server.name);
  for (const server of known) {
    if (!options.has(server.id)) options.set(server.id, server.name);
  }
  if (options.size === 0) {
    return <Note>Join a server and it shows up here.</Note>;
  }
  const full = picked.length >= MAX_SERVERS_SHOWN;
  return (
    <div className="flex flex-col gap-2">
      <p className="text-ui-xs text-ink-3">
        {picked.length} of {MAX_SERVERS_SHOWN} picked.
      </p>
      <ul className="flex flex-col gap-0.5">
        {[...options].map(([id, name]) => {
          const on = picked.includes(id);
          return (
            <ToggleRow
              key={id}
              pressed={on}
              disabled={!on && full}
              onClick={() =>
                onChange(
                  on ? picked.filter((item) => item !== id) : [...picked, id],
                )
              }
            >
              {name}
            </ToggleRow>
          );
        })}
      </ul>
    </div>
  );
}
