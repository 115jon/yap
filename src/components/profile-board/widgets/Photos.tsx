/**
 * Photo strip: one to four of the owner's pictures, cropped into fixed
 * boxes so the plate never changes shape, with an optional caption. GIFs
 * hold still until hovered or focused (WidgetImage).
 */

import { Num } from "@/components/ui/num";
import { cn } from "@/lib/utils";

import { useBoardMedia } from "../board-context";
import { Plate, WidgetEmpty, WidgetImage, type WidgetProps } from "./shared";

export function PhotosWidget({ widget, ctx }: WidgetProps<"photos">) {
  const media = useBoardMedia();
  const { photos, caption } = widget.cfg;
  const limit = widget.size === "S" ? 1 : 4;
  const shown = photos.slice(0, limit);
  const readout =
    photos.length > 0 ? (
      <Num className="text-ink-3">{photos.length}</Num>
    ) : null;

  if (shown.length === 0) {
    return (
      <Plate widget={widget} ctx={ctx}>
        <WidgetEmpty
          icon="image"
          title={ctx.isOwner ? "No photos yet" : "No photos"}
          hint={
            ctx.isOwner && widget.size !== "S"
              ? "Add up to 4 in Edit board."
              : undefined
          }
        />
      </Plate>
    );
  }

  const grid =
    widget.size === "L"
      ? { gridTemplateColumns: `repeat(${shown.length}, minmax(0, 1fr))` }
      : widget.size === "M" && shown.length > 2
        ? {
            gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
            gridTemplateRows: "repeat(2, minmax(0, 1fr))",
          }
        : { gridTemplateRows: `repeat(${shown.length}, minmax(0, 1fr))` };

  return (
    <Plate widget={widget} ctx={ctx} readout={readout}>
      <ul
        aria-label="Photos"
        className="grid min-h-0 flex-1 gap-1"
        style={grid}
      >
        {shown.map((photo, index) => (
          <li
            key={photo.asset}
            className={cn(
              "min-h-0 min-w-0",
              widget.size === "M" &&
                shown.length === 3 &&
                index === 0 &&
                "col-span-2",
            )}
          >
            <WidgetImage
              src={media.asset(ctx.ownerId, photo.asset)}
              alt={photo.alt}
              gif={photo.asset.endsWith(".gif")}
              className="size-full"
            />
          </li>
        ))}
      </ul>
      {caption && widget.size !== "S" ? (
        <p className="line-clamp-2 text-ui-xs text-pretty text-ink-3">
          {caption}
        </p>
      ) : null}
    </Plate>
  );
}
