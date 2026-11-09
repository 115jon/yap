/**
 * Custom: the paid plate (perk `customWidgets`). The owner names the
 * legend, writes a title and up to four short lines, and adds a picture.
 * Without the perk only the owner sees it, tagged "Draft · only you".
 */

import { cn } from "@/lib/utils";

import { useBoardMedia } from "../board-context";
import {
  Plate,
  WidgetEmpty,
  WidgetImage,
  WidgetTitle,
  type WidgetProps,
} from "./shared";

export function CustomWidget({ widget, ctx }: WidgetProps<"custom">) {
  const media = useBoardMedia();
  const { legend, title, asset, lines } = widget.cfg;
  const filled = lines.filter((line) => line.trim());
  const image = asset ? (
    <WidgetImage
      src={media.asset(ctx.ownerId, asset)}
      alt=""
      gif={asset.endsWith(".gif")}
      className="size-full"
    />
  ) : null;

  if (!title && !image && filled.length === 0) {
    return (
      <Plate widget={widget} ctx={ctx} legend={legend}>
        <WidgetEmpty
          icon="sparkle"
          title={ctx.isOwner ? "Make it yours" : "Nothing here yet"}
          hint={
            ctx.isOwner
              ? "Add a title, a picture and up to four lines in Edit board."
              : undefined
          }
        />
      </Plate>
    );
  }

  const text = (max: number) => (
    <>
      {title ? (
        <WidgetTitle
          className={cn(
            "line-clamp-2 text-balance",
            widget.size !== "S" && "text-[18px] leading-[22px]",
          )}
        >
          {title}
        </WidgetTitle>
      ) : null}
      {filled.length > 0 ? (
        <ul className="flex min-w-0 flex-col gap-1">
          {filled.slice(0, max).map((line, index) => (
            <li
              key={index}
              className="flex min-w-0 items-baseline gap-2 text-ui-sm text-ink-2"
            >
              <span
                aria-hidden="true"
                className="h-px w-1.5 shrink-0 -translate-y-1 bg-ink-3"
              />
              <span className="min-w-0 break-words">{line}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </>
  );

  if (widget.size === "S") {
    return (
      <Plate widget={widget} ctx={ctx} legend={legend}>
        {image && !filled.length ? (
          <>
            <span className="block min-h-0 flex-1">{image}</span>
            {title ? (
              <WidgetTitle className="truncate">{title}</WidgetTitle>
            ) : null}
          </>
        ) : (
          <div className="flex min-h-0 min-w-0 gap-3">
            {image ? (
              <span className="size-14 shrink-0 @max-[200px]/cell:hidden">
                {image}
              </span>
            ) : null}
            <div className="flex min-w-0 flex-col gap-1.5">{text(3)}</div>
          </div>
        )}
      </Plate>
    );
  }

  if (widget.size === "M") {
    return (
      <Plate widget={widget} ctx={ctx} legend={legend}>
        {image ? (
          <span className="block aspect-[4/3] w-full shrink-0">{image}</span>
        ) : null}
        <div className="flex min-h-0 min-w-0 flex-col gap-1.5">{text(4)}</div>
      </Plate>
    );
  }

  return (
    <Plate widget={widget} ctx={ctx} legend={legend}>
      <div className="flex min-h-0 flex-1 gap-4">
        {image ? (
          <span className="aspect-[4/3] h-full shrink-0 @max-[420px]/cell:aspect-square">
            {image}
          </span>
        ) : null}
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">{text(4)}</div>
      </div>
    </Plate>
  );
}
