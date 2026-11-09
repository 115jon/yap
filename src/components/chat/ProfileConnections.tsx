/**
 * The profile's Connections block: one key per linked account the owner
 * shows, its provider glyph and name, linking out to the account's profile
 * in a new tab (the desktop app's link handler takes it to the browser).
 * Only what the profile API gives viewers: provider, name and profile URL.
 */

import { Icon } from "@/components/ui/icon";
import {
  CONNECTION_PROVIDER_INFO,
  isConnectionProvider,
  type PublicConnection,
} from "@/lib/connections";
import { cn } from "@/lib/utils";

const KEY =
  "flex h-8 min-w-0 max-w-full items-center gap-2 rounded-[2px] bg-tint-1 pr-2.5 pl-2 text-ui-sm text-ink-1";

/** Only a web profile URL is a link; anything else prints as text. */
function safeProfileUrl(raw: string | null): string | null {
  if (!raw) return null;
  try {
    const url = new URL(raw);
    return url.protocol === "https:" ? url.toString() : null;
  } catch {
    return null;
  }
}

export function ProfileConnectionList({
  connections,
}: {
  connections: PublicConnection[];
}) {
  return (
    <ul className="flex flex-wrap gap-1.5" data-profile-connections="">
      {connections.map((connection) => {
        if (!isConnectionProvider(connection.provider)) return null;
        const info = CONNECTION_PROVIDER_INFO[connection.provider];
        const href = safeProfileUrl(connection.profile_url);
        const content = (
          <>
            <Icon name={info.icon} size="sm" className="shrink-0 text-ink-2" />
            <span className="sr-only">{info.label}: </span>
            <span className="min-w-0 truncate">{connection.display_name}</span>
          </>
        );
        return (
          <li key={connection.provider} className="flex min-w-0 max-w-full">
            {href ? (
              <a
                href={href}
                target="_blank"
                rel="noopener noreferrer"
                aria-label={`${info.label}: ${connection.display_name} (opens in a new tab)`}
                title={`${connection.display_name} on ${info.label}`}
                className={cn(
                  KEY,
                  "relative transition-colors duration-(--dur-1) ease-out pointer-fine:hover:bg-tint-2 active:bg-tint-3",
                  "before:absolute before:top-1/2 before:left-1/2 before:h-full before:min-h-11 before:w-full before:-translate-x-1/2 before:-translate-y-1/2 before:content-[''] pointer-fine:before:hidden",
                )}
              >
                {content}
                <Icon
                  name="externalLink"
                  size="xs"
                  className="shrink-0 text-ink-3"
                />
              </a>
            ) : (
              <span className={KEY}>{content}</span>
            )}
          </li>
        );
      })}
    </ul>
  );
}
