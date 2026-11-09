/**
 * Links: up to five labelled links. Every one leaves Yap through the
 * external-link confirmation, and carries `rel="noopener noreferrer ugc"`:
 * the owner wrote them, not Yap.
 */

import type { MouseEvent } from "react";

import { Icon } from "@/components/ui/icon";
import { useExternalLinkConfirmation } from "@/hooks/useExternalLinkConfirmation";
import { linkHost } from "@/lib/profile-board";

import { Plate, WidgetEmpty, type WidgetProps } from "./shared";

export function LinksWidget({ widget, ctx }: WidgetProps<"links">) {
  const { requestOpen, confirmation } = useExternalLinkConfirmation();
  const links = widget.cfg.links;
  const limit = widget.size === "S" ? 3 : 5;
  const shown = links.slice(0, limit);
  const more = links.length - shown.length;

  if (shown.length === 0) {
    return (
      <Plate widget={widget} ctx={ctx}>
        <WidgetEmpty
          icon="link"
          title={ctx.isOwner ? "No links yet" : "No links"}
        />
      </Plate>
    );
  }

  const open = (event: MouseEvent<HTMLAnchorElement>, url: string) => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey)
      return;
    event.preventDefault();
    requestOpen(url);
  };

  return (
    <Plate widget={widget} ctx={ctx}>
      <ul aria-label="Links" className="-mx-2 flex flex-col">
        {shown.map((link) => {
          const host = linkHost(link.url);
          return (
            <li key={link.url}>
              <a
                href={link.url}
                target="_blank"
                rel="noopener noreferrer ugc"
                onClick={(event) => open(event, link.url)}
                className="group/link flex h-9 min-w-0 items-center gap-2.5 rounded-[2px] px-2 text-ink-1 outline-none transition-colors duration-(--dur-1) focus-visible:ring-2 focus-visible:ring-accent-ink pointer-fine:hover:bg-tint-1"
              >
                <Icon name="link" size="sm" className="shrink-0 text-ink-3" />
                <span className="min-w-0 flex-1 truncate text-ui-row font-medium">
                  {link.label}
                </span>
                {host && host !== link.label ? (
                  <span className="max-w-[45%] shrink truncate text-num-xs text-ink-3 @max-[200px]/cell:hidden">
                    {host}
                  </span>
                ) : null}
                <Icon
                  name="externalLink"
                  size="xs"
                  className="shrink-0 text-ink-3 transition-colors duration-(--dur-1) group-hover/link:text-ink-1"
                />
              </a>
            </li>
          );
        })}
      </ul>
      {more > 0 ? (
        <p className="mt-auto text-ui-xs text-ink-3">
          +{more} more {more === 1 ? "link" : "links"}
        </p>
      ) : null}
      {confirmation}
    </Plate>
  );
}
