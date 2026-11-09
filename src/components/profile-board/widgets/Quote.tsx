/**
 * Pinned message: a snapshot of a message the owner sent, as it was when
 * pinned. A viewer outside the message's server sees that it exists, not
 * what it says.
 */

import { Avatar } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Num } from "@/components/ui/num";
import { cn } from "@/lib/utils";

import { useBoardMedia } from "../board-context";
import { Plate, WidgetEmpty, type WidgetProps } from "./shared";

const DAY_TIME = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
});
const CLOCK = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
});

const CLAMP = {
  S: "line-clamp-3",
  M: "line-clamp-[9]",
  L: "line-clamp-2",
} as const;

export function QuoteWidget({ widget, data, ctx }: WidgetProps<"quote">) {
  const media = useBoardMedia();
  const snapshot = widget.cfg.snapshot;
  const readout = (
    <Icon name="pin" state="active" size="xs" className="text-ink-3" />
  );

  if (!snapshot || !widget.cfg.message_id) {
    return (
      <Plate widget={widget} ctx={ctx}>
        <WidgetEmpty
          icon="pin"
          title={ctx.isOwner ? "Nothing pinned yet" : "Nothing pinned"}
          hint={
            ctx.isOwner
              ? "Pin a message you sent from its menu or in Edit board."
              : undefined
          }
        />
      </Plate>
    );
  }

  if (!ctx.isOwner && data.quote_visible === false) {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <WidgetEmpty
          icon="lock"
          title="Message from a server you're not in"
          hint={
            widget.size === "S"
              ? undefined
              : "Only members of that server can read it."
          }
        />
      </Plate>
    );
  }

  const at = Date.parse(snapshot.at);
  const where = [
    snapshot.channel_name ? `#${snapshot.channel_name}` : null,
    snapshot.server_name,
  ]
    .filter(Boolean)
    .join(" · ");
  const messageId = widget.cfg.message_id;

  return (
    <Plate widget={widget} ctx={ctx} readout={readout}>
      <div className="flex min-w-0 items-center gap-2">
        <Avatar
          name={ctx.ownerName}
          src={ctx.ownerAvatarUrl ? media.url(ctx.ownerAvatarUrl) : null}
          size={24}
          className="shrink-0"
        />
        <span className="truncate font-display text-[15px] leading-5 font-bold text-ink-1">
          {ctx.ownerName}
        </span>
        {Number.isFinite(at) ? (
          <Num className="shrink-0 text-ink-3 @max-[200px]/cell:hidden">
            <time dateTime={snapshot.at}>
              {DAY_TIME.format(at)} · {CLOCK.format(at)}
            </time>
          </Num>
        ) : null}
      </div>
      <blockquote
        className={cn(
          "min-h-0 text-ui-base break-words text-pretty text-ink-1",
          widget.size === "S" && "text-ui-sm",
          CLAMP[widget.size],
        )}
        title={snapshot.text}
      >
        {snapshot.text}
      </blockquote>
      <div className="mt-auto flex min-w-0 items-center gap-2">
        <p className="min-w-0 flex-1 truncate text-ui-xs text-ink-3">{where}</p>
        {ctx.onOpenMessage ? (
          <Button
            variant="link"
            size="sm"
            className="shrink-0 text-ui-sm"
            onClick={() => ctx.onOpenMessage?.(messageId, snapshot)}
          >
            Jump
          </Button>
        ) : null}
      </div>
    </Plate>
  );
}
