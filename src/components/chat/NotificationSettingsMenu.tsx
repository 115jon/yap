import type { ContextMenuItem } from "@/components/chat/ContextMenu";
import {
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import { Icon } from "@/components/ui/icon";
import {
  MENU_ITEM,
  MENU_ITEM_DEFAULT,
  MENU_LABEL,
  MENU_SEPARATOR,
} from "@/components/ui/menu";
import { isMobileViewport, useIsMobile } from "@/hooks/useMediaQuery";
import {
  NOTIFICATION_LEVELS,
  type MuteDuration,
  type NotificationLevel,
} from "@/lib/notification-levels";
import {
  INHERIT_LEVEL,
  MUTE_DURATION_LABELS,
  MUTE_DURATIONS,
  NOTIFICATION_LEVEL_LABELS,
  notificationMenuView,
  targetTypeAndId,
  type LevelChoice,
  type NotificationMenuView,
  type NotificationTarget,
} from "@/lib/notification-settings-client";
import { cn } from "@/lib/utils";
import { useChatStore } from "@/stores/chat-store";
import { useState } from "react";

/**
 * The "Notifications" row for a server, channel or DM context menu: a flyout
 * on desktop, an inline expansion in the phone sheet. Both hold the level
 * radios and the mute choices, and both read the store live.
 */
export function notificationsMenuItem(
  target: NotificationTarget,
  options: { divider?: boolean } = {},
): ContextMenuItem {
  const state = useChatStore.getState();
  const view = notificationMenuView(
    state.notificationSettings,
    target,
    Math.max(Date.now(), state.notificationSettingsNow),
  );
  return {
    key: "notifications",
    label: "Notifications",
    // The sheet keeps its rows while the expansion changes the setting, so
    // a summary there would go stale; the expansion says it instead.
    subtitle: isMobileViewport() ? undefined : view.summary,
    icon: <Icon name={view.effectivelyMuted ? "bellOff" : "bell"} size="sm" />,
    onClick: () => {},
    closeOnClick: false,
    submenu: <NotificationSettingsPanel target={target} />,
    divider: options.divider,
  };
}

function useNotificationMenu(target: NotificationTarget) {
  const settings = useChatStore((s) => s.notificationSettings);
  const clock = useChatStore((s) => s.notificationSettingsNow);
  const update = useChatStore((s) => s.actions.updateNotificationSettings);
  // The panel mounts as the menu opens: that is "now" for its labels.
  const [openedAt] = useState(() => Date.now());
  const view = notificationMenuView(
    settings,
    target,
    Math.max(openedAt, clock),
  );
  const { targetType, targetId } = targetTypeAndId(target);
  return {
    view,
    setLevel: (choice: string) => {
      if (
        choice !== INHERIT_LEVEL &&
        !NOTIFICATION_LEVELS.includes(choice as never)
      )
        return;
      void update({
        target_type: targetType,
        target_id: targetId,
        level: choice === INHERIT_LEVEL ? null : (choice as NotificationLevel),
      });
    },
    setMute: (mute: MuteDuration | "off") =>
      void update({ target_type: targetType, target_id: targetId, mute }),
  };
}

function levelChoices(target: NotificationTarget): LevelChoice[] {
  return target.kind === "channel"
    ? [INHERIT_LEVEL, ...NOTIFICATION_LEVELS]
    : [...NOTIFICATION_LEVELS];
}

function ChoiceLabel({
  choice,
  serverLevel,
}: {
  choice: LevelChoice;
  serverLevel: NotificationLevel;
}) {
  if (choice !== INHERIT_LEVEL) return <>{NOTIFICATION_LEVEL_LABELS[choice]}</>;
  return (
    <span className="flex min-w-0 flex-col">
      <span className="truncate">Use server default</span>
      <span className="truncate text-ui-xs font-normal text-ink-2">
        {NOTIFICATION_LEVEL_LABELS[serverLevel]}
      </span>
    </span>
  );
}

function muteHeading(view: NotificationMenuView): string {
  return view.mutedLabel ?? view.serverMutedLabel ?? "Mute";
}

export function NotificationSettingsPanel({
  target,
}: {
  target: NotificationTarget;
}) {
  const isMobile = useIsMobile();
  return isMobile ? (
    <SheetPanel target={target} />
  ) : (
    <FlyoutPanel target={target} />
  );
}

/** Desktop: Base UI rows inside the submenu panel. */
function FlyoutPanel({ target }: { target: NotificationTarget }) {
  const { view, setLevel, setMute } = useNotificationMenu(target);
  return (
    <>
      <DropdownMenuGroup>
        <DropdownMenuLabel>Notify me about</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={view.level}
          onValueChange={(value) => setLevel(String(value))}
        >
          {levelChoices(target).map((choice) => (
            <DropdownMenuRadioItem
              key={choice}
              value={choice}
              closeOnClick
              className={cn(choice === INHERIT_LEVEL && "min-h-11")}
            >
              <ChoiceLabel choice={choice} serverLevel={view.serverLevel} />
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuGroup>
      <DropdownMenuSeparator />
      <DropdownMenuGroup>
        <DropdownMenuLabel>{muteHeading(view)}</DropdownMenuLabel>
        {view.muted ? (
          <DropdownMenuItem
            icon={<Icon name="bell" size="sm" />}
            onClick={() => setMute("off")}
          >
            Unmute
          </DropdownMenuItem>
        ) : (
          MUTE_DURATIONS.map((duration) => (
            <DropdownMenuItem key={duration} onClick={() => setMute(duration)}>
              {MUTE_DURATION_LABELS[duration]}
            </DropdownMenuItem>
          ))
        )}
      </DropdownMenuGroup>
    </>
  );
}

/** Phones: the same choices as plain rows in the sheet's expansion. */
function SheetPanel({ target }: { target: NotificationTarget }) {
  const { view, setLevel, setMute } = useNotificationMenu(target);
  const row = cn(MENU_ITEM, MENU_ITEM_DEFAULT, "min-h-11");
  return (
    <div className="flex flex-col">
      <div role="radiogroup" aria-label="Notify me about">
        <p aria-hidden className={MENU_LABEL}>
          Notify me about
        </p>
        {levelChoices(target).map((choice) => {
          const selected = view.level === choice;
          return (
            <button
              key={choice}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => setLevel(choice)}
              className={row}
            >
              <span className="min-w-0 flex-1">
                <ChoiceLabel choice={choice} serverLevel={view.serverLevel} />
              </span>
              {selected ? (
                <Icon name="check" size="sm" className="shrink-0" />
              ) : null}
            </button>
          );
        })}
      </div>
      <div aria-hidden className={MENU_SEPARATOR} />
      <div role="group" aria-label={muteHeading(view)}>
        <p aria-hidden className={MENU_LABEL}>
          {muteHeading(view)}
        </p>
        {view.muted ? (
          <button type="button" onClick={() => setMute("off")} className={row}>
            <Icon name="bell" size="sm" className="shrink-0 text-ink-2" />
            <span className="min-w-0 flex-1 truncate">Unmute</span>
          </button>
        ) : (
          MUTE_DURATIONS.map((duration) => (
            <button
              key={duration}
              type="button"
              onClick={() => setMute(duration)}
              className={row}
            >
              <span className="min-w-0 flex-1 truncate">
                {MUTE_DURATION_LABELS[duration]}
              </span>
            </button>
          ))
        )}
      </div>
    </div>
  );
}
