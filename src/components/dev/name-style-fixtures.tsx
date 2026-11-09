/**
 * Dev-only: display name styles on every surface a styled name appears.
 *
 *   /ui-fixtures?surface=name-styles            every effect x every surface
 *   &wave=1                                     ...with the Wave modifier
 *   &play=1                                     ...playing (as in a profile)
 *   ?surface=name-style-editor                  the style dialog (see below)
 *
 * Each row is one effect; each column copies one real call site's markup
 * and classes (the member row, a voice occupant, a message author, a
 * mention row, a reply line, the profile card name at 18/22/28px), with a
 * short name, one with descenders and one long enough to truncate. Every
 * name is a `.rm-name` the capture script measures for clipping.
 *
 * `&name=<effect>` (with `&wave=1`) on ANY fixture URL dresses every
 * fixture user in that style instead, so the real surfaces (shell, members,
 * account-row, ov-profile, mobile-profile, search, mention) show it.
 */

import type { ReactNode } from "react";

import { ProfileDisplayName } from "@/components/chat/ProfileDisplayName";
import { DECORATION_PLAY_ATTR } from "@/components/chat/ProfileAssetLayer";
import { DisplayNameStyleDialog } from "@/components/chat/SettingsAccountTab";
import { UserDisplayName } from "@/components/chat/UserDisplayName";
import { MENU_ITEM } from "@/components/ui/menu";
import { OCCUPANT_ROW, sidebarRowClass } from "@/components/ui/sidebar-row";
import {
  DISPLAY_NAME_EFFECT_OPTIONS,
  resolveProfileTheme,
  type DisplayNameEffectId,
  type DisplayNameStyle,
} from "@/lib/profile-customization";
import type { User } from "@/lib/types";
import { cn } from "@/lib/utils";

/** Colours each effect is shown in (the editor's swatches). */
const FIXTURE_COLORS: Record<DisplayNameEffectId, [string, string]> = {
  solid: ["#F8FAFC", "#FB7185"],
  gradient: ["#F472B6", "#38BDF8"],
  neon: ["#38BDF8", "#C084FC"],
  toon: ["#FDE68A", "#F97316"],
  pop: ["#4ADE80", "#38BDF8"],
  dither: ["#F472B6", "#38BDF8"],
  holo: ["#F8FAFC", "#FB7185"],
  chrome: ["#F8FAFC", "#FB7185"],
  pixel: ["#FDE68A", "#FFFFFF"],
  crt: ["#4ADE80", "#4ADE80"],
  glitch: ["#F8FAFC", "#FB7185"],
  ember: ["#F97316", "#FDE68A"],
  sticker: ["#FFFFFF", "#F472B6"],
  candlelit: ["#FDE68A", "#F97316"],
};

export function fixtureNameStyle(
  effect: DisplayNameEffectId,
  wave = false,
): DisplayNameStyle {
  const option = DISPLAY_NAME_EFFECT_OPTIONS.find((o) => o.id === effect);
  const [primaryColor, secondaryColor] = FIXTURE_COLORS[effect];
  return {
    font: option?.font ?? "gg-sans",
    effect,
    primaryColor,
    secondaryColor,
    ...(wave ? { wave: true } : {}),
  };
}

function isEffect(value: string | null): value is DisplayNameEffectId {
  return DISPLAY_NAME_EFFECT_OPTIONS.some((o) => o.id === value);
}

/** The `&name=` / `&wave=` style from the URL (module-load fixtures). */
export function nameStyleFromLocation(): DisplayNameStyle | null {
  if (typeof window === "undefined") return null;
  const params = new URLSearchParams(window.location.search);
  const effect = params.get("name");
  return isEffect(effect)
    ? fixtureNameStyle(effect, params.get("wave") === "1")
    : null;
}

/** `&name=<effect>[&wave=1]`: dress the fixture users in place. */
export function applyNameStyleParam(
  users: Array<User | undefined>,
  effect: string | null,
  wave: boolean,
) {
  if (!isEffect(effect)) return;
  const style = fixtureNameStyle(effect, wave);
  for (const user of users) if (user) user.display_name_style = style;
}

const NAMES = ["Peggy Quayle", "Ada", "Margaret Hamilton-Quigley Junior"];

function Avatar({ size }: { size: number }) {
  return (
    <span
      aria-hidden="true"
      className="shrink-0 rounded-avatar bg-tint-3"
      style={{ width: size, height: size }}
    />
  );
}

/** MemberList's row: 40 tall, overflow hidden (nameplates), name over status. */
function MemberSpecimen({ name, style }: SpecimenProps) {
  return (
    <div role="listitem" className="w-[232px] bg-ply-0 p-1">
      <button
        type="button"
        className={cn(
          sidebarRowClass({ tall: true }),
          "w-full min-w-0 flex-1 cursor-pointer gap-3 overflow-clip pl-2.5 [overflow-clip-margin:8px]",
        )}
      >
        <Avatar size={32} />
        <span className="relative z-10 flex min-w-0 flex-1 flex-col">
          <span className="flex min-w-0 items-center gap-1.5">
            <span className="min-w-0 shrink">
              <UserDisplayName
                text={name}
                displayNameStyle={style}
                className="block min-w-0 truncate"
              />
            </span>
          </span>
          <span className="truncate text-ui-xs font-normal text-ink-3">
            shipping the UI pass
          </span>
        </span>
      </button>
    </div>
  );
}

/** ChannelSidebar's voice occupant: a 28px row, 24px avatar. */
function OccupantSpecimen({ name, style }: SpecimenProps) {
  return (
    <div className="w-[232px] bg-ply-1 p-1">
      <button type="button" className={cn(OCCUPANT_ROW, "-ml-1 pl-1")}>
        <span className="relative size-6 shrink-0 rounded-avatar bg-tint-3" />
        <UserDisplayName
          text={name}
          displayNameStyle={style}
          className="min-w-0 flex-1 truncate"
        />
      </button>
    </div>
  );
}

/** MessageItem's header: Bricolage 15/20 author, then the time. */
function AuthorSpecimen({ name, style }: SpecimenProps) {
  return (
    <div className="group/row flex w-[300px] gap-3 bg-ply-0 px-2 py-1">
      <Avatar size={40} />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-baseline gap-2">
          <button
            type="button"
            className="min-w-0 cursor-pointer border-0 bg-transparent p-0 text-left font-display-tuned text-[15px] leading-5 font-bold text-ink-1 outline-none [&_*]:[animation-play-state:paused] pointer-fine:group-hover/row:[&_*]:[animation-play-state:running]"
          >
            <ProfileDisplayName
              text={name}
              displayNameStyle={style}
              className="block truncate font-display-tuned text-[15px] leading-5 font-bold"
            />
          </button>
          <span className="shrink-0 text-num-sm text-ink-3">14:32</span>
        </div>
        <p className="text-ui-base text-ink-1">Pushing the captures now.</p>
      </div>
    </div>
  );
}

/** MentionAutocomplete's option row inside the menu panel. */
function MentionSpecimen({ name, style }: SpecimenProps) {
  return (
    <div className="w-[260px] border border-line-strong bg-ply-3 p-1">
      <div role="option" aria-selected className={cn(MENU_ITEM, "text-ink-1")}>
        <span className="flex size-6 shrink-0 rounded-avatar bg-tint-3" />
        <UserDisplayName
          text={name}
          displayNameStyle={style}
          className="block min-w-0 truncate text-ui-row"
        />
        <span className="ms-auto shrink-0 pl-3 text-ui-xs font-normal text-ink-3">
          @peggy
        </span>
      </div>
    </div>
  );
}

/** The reply line over a message (12px, capped at 160px, always still). */
function ReplySpecimen({ name, style }: SpecimenProps) {
  return (
    <div className="relative flex h-5 w-[260px] items-center bg-ply-0 pr-6 pl-2">
      <button
        type="button"
        className="relative flex max-w-[72ch] min-w-0 cursor-pointer items-center gap-1.5 border-0 bg-transparent p-0 text-left text-ui-xs text-ink-3"
      >
        <span className="relative flex size-4 shrink-0 bg-tint-3" />
        <ProfileDisplayName
          text={name}
          displayNameStyle={style}
          className="block max-w-40 shrink-0 truncate text-ui-xs font-semibold text-ink-2 [&_*]:[animation-play-state:paused]"
        />
        <span className="min-w-0 flex-1 truncate">that one</span>
      </button>
    </div>
  );
}

/** ProfileCard's name line (popover 18, modal/sheet 22, plus 28). */
function ProfileSpecimen({
  name,
  style,
  size,
}: SpecimenProps & { size: string }) {
  return (
    <div
      className="w-[300px] bg-ply-3 px-4 py-3"
      {...{ [DECORATION_PLAY_ATTR]: "" }}
    >
      <div className="flex min-w-0 flex-col gap-1">
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          <ProfileDisplayName
            text={name}
            displayNameStyle={style}
            className={cn("min-w-0 truncate font-semibold text-ink-1", size)}
          />
        </div>
        <p className="text-ui-sm text-ink-2">@peggy</p>
      </div>
    </div>
  );
}

interface SpecimenProps {
  name: string;
  style: DisplayNameStyle;
}

const SURFACES: Array<{
  id: string;
  label: string;
  render: (props: SpecimenProps) => ReactNode;
}> = [
  {
    id: "member",
    label: "Member row 14",
    render: (p) => <MemberSpecimen {...p} />,
  },
  {
    id: "occupant",
    label: "Voice 13",
    render: (p) => <OccupantSpecimen {...p} />,
  },
  {
    id: "author",
    label: "Author 15",
    render: (p) => <AuthorSpecimen {...p} />,
  },
  {
    id: "mention",
    label: "Mention 14",
    render: (p) => <MentionSpecimen {...p} />,
  },
  { id: "reply", label: "Reply 12", render: (p) => <ReplySpecimen {...p} /> },
  {
    id: "profile-18",
    label: "Profile 18",
    render: (p) => <ProfileSpecimen {...p} size="text-ui-lg" />,
  },
  {
    id: "profile-22",
    label: "Profile 22",
    render: (p) => <ProfileSpecimen {...p} size="text-ui-xl" />,
  },
  {
    id: "profile-28",
    label: "Profile 28",
    render: (p) => <ProfileSpecimen {...p} size="text-ui-2xl" />,
  },
];

export function NameStylesFixture({
  wave,
  play,
}: {
  wave: boolean;
  play: boolean;
}) {
  return (
    <div
      className="h-full w-full overflow-auto bg-ply-0 p-4 text-ink-1"
      {...(play ? { [DECORATION_PLAY_ATTR]: "" } : {})}
    >
      <div className="flex flex-col gap-6">
        {DISPLAY_NAME_EFFECT_OPTIONS.map((option) => {
          const style = fixtureNameStyle(option.id, wave);
          return (
            <section
              key={option.id}
              data-testid={`ns-${option.id}`}
              className="flex flex-col gap-2 border-t-2 border-ink-1 pt-2"
            >
              <h2 className="silkscreen text-ink-3">
                {option.group} · {option.label}
                {wave ? " · wave" : ""}
              </h2>
              <div className="flex flex-wrap gap-x-4 gap-y-3">
                {SURFACES.map((surface) => (
                  <div
                    key={surface.id}
                    data-ns-surface={surface.id}
                    className="flex flex-col gap-1"
                  >
                    <span className="silkscreen text-ink-3">
                      {surface.label}
                    </span>
                    {NAMES.map((name) => (
                      <div key={name}>{surface.render({ name, style })}</div>
                    ))}
                  </div>
                ))}
              </div>
            </section>
          );
        })}
      </div>
    </div>
  );
}

/**
 * `?surface=name-style-editor`: the style dialog, open. `&name=<effect>`
 * (and `&wave=1`) starts it on that style; `&locked=1` renders it for an
 * account without the Yap styles (`canUseYapStyles={false}`).
 */
export function NameStyleEditorFixture() {
  const params = new URLSearchParams(window.location.search);
  const theme = resolveProfileTheme({});
  return (
    <div className="h-full w-full bg-ply-0">
      <DisplayNameStyleDialog
        open
        displayName="Peggy Quayle"
        username="peggy"
        initialStyle={nameStyleFromLocation()}
        themeStyle={theme.variables}
        themeBackgroundColor={theme.backgroundColor}
        themeTextColor={theme.textColor}
        canUseYapStyles={params.get("locked") !== "1"}
        onClose={() => {}}
        onApply={() => {}}
      />
    </div>
  );
}
