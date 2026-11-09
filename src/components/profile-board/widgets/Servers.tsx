/**
 * Favourite servers: up to 4 of the owner's servers with their size, and
 * whether the viewer is in each too.
 */

import { Icon } from "@/components/ui/icon";
import { Num } from "@/components/ui/num";
import type { BoardServer } from "@/lib/profile-board";
import { cn } from "@/lib/utils";

import { useBoardMedia } from "../board-context";
import { Plate, WidgetEmpty, WidgetImage, type WidgetProps } from "./shared";

export function ServersWidget({ widget, data, ctx }: WidgetProps<"servers">) {
  const servers = data.servers ?? [];

  if (servers.length === 0) {
    return (
      <Plate widget={widget} ctx={ctx}>
        <WidgetEmpty
          icon="server"
          title={ctx.isOwner ? "No servers picked" : "No servers to show"}
          hint={
            ctx.isOwner && widget.size !== "S"
              ? "Pick up to 4 in Edit board."
              : undefined
          }
        />
      </Plate>
    );
  }

  if (widget.size === "S") {
    const first = servers[0]!;
    return (
      <Plate
        widget={widget}
        ctx={ctx}
        readout={<Num className="text-ink-3">{servers.length}</Num>}
      >
        <ul aria-label="Servers" className="flex items-center gap-2">
          {servers.slice(0, 4).map((server, index) => (
            <li
              key={server.id}
              title={server.name}
              className={cn(index === 3 && "@max-[200px]/cell:hidden")}
            >
              <ServerTile server={server} className="size-10" />
              <span className="sr-only">{server.name}</span>
            </li>
          ))}
        </ul>
        <div className="mt-auto flex min-w-0 flex-col gap-0.5">
          <p className="truncate text-ui-sm font-semibold text-ink-1">
            {first.name}
          </p>
          <p className="truncate text-ui-xs text-ink-3">
            <Num className="text-ink-2">{first.member_count}</Num> members
          </p>
        </div>
      </Plate>
    );
  }

  return (
    <Plate widget={widget} ctx={ctx}>
      <ul
        aria-label="Servers"
        className={cn(
          "grid gap-x-4 gap-y-3",
          widget.size === "L"
            ? "grid-cols-2 @max-[420px]/cell:grid-cols-1 @max-[420px]/cell:[&>li:nth-child(n+3)]:hidden"
            : "grid-cols-1",
        )}
      >
        {servers.slice(0, 4).map((server) => (
          <li key={server.id} className="flex min-w-0 items-center gap-2.5">
            <ServerTile server={server} className="size-10" />
            <div className="flex min-w-0 flex-1 flex-col gap-0.5">
              <span
                className="truncate text-ui-sm font-semibold text-ink-1"
                title={server.name}
              >
                {server.name}
              </span>
              <span className="flex min-w-0 items-center gap-1 truncate text-ui-xs text-ink-3">
                <Icon name="members" size="xs" className="shrink-0" />
                <Num className="text-ink-2">{server.member_count}</Num>
                {server.viewer_is_member ? (
                  <span className="truncate @max-[200px]/cell:hidden">
                    {" "}
                    · You're in it
                  </span>
                ) : null}
              </span>
            </div>
          </li>
        ))}
      </ul>
    </Plate>
  );
}

function ServerTile({
  server,
  className,
}: {
  server: BoardServer;
  className?: string;
}) {
  const media = useBoardMedia();
  if (server.icon_url) {
    return (
      <WidgetImage
        src={media.url(server.icon_url)}
        alt=""
        className={cn("shrink-0", className)}
      />
    );
  }
  return (
    <span
      aria-hidden="true"
      className={cn(
        "flex shrink-0 items-center justify-center rounded-[2px] bg-tint-3 font-display text-[15px] font-extrabold text-ink-1",
        className,
      )}
    >
      {server.name.slice(0, 1).toUpperCase()}
    </span>
  );
}
