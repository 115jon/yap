/**
 * Landing back from a provider's sign-in. The callback redirects to
 * `/chat?connection=<provider>&connection_status=<status>`; this opens
 * Settings › Connections, says how it went in a toast, and takes the
 * parameters out of the address bar so a reload doesn't repeat it.
 * `/chat?settings=connections` (the desktop app's "Connect in browser")
 * opens the panel without a toast.
 */

import { openSettings } from "@/components/chat/settings/settings-links";
import { toast } from "@/components/ui/toast";
import {
  CONNECTION_RESULT_PARAM,
  CONNECTION_STATUS_PARAM,
  describeConnectionResult,
  isConnectionProvider,
  isConnectionResultStatus,
  type ConnectionProvider,
  type ConnectionResultStatus,
} from "@/lib/connections";
import { useEffect } from "react";

const SETTINGS_PARAM = "settings";

export type ConnectionReturnIntent =
  | {
      kind: "result";
      provider: ConnectionProvider;
      status: ConnectionResultStatus;
    }
  | { kind: "open" };

/** What the address asks for, and the address without it; null if nothing. */
export function readConnectionReturn(
  href: string,
): { intent: ConnectionReturnIntent; cleaned: string } | null {
  const url = new URL(href);
  const provider = url.searchParams.get(CONNECTION_RESULT_PARAM);
  const status = url.searchParams.get(CONNECTION_STATUS_PARAM);
  const settings = url.searchParams.get(SETTINGS_PARAM);
  let intent: ConnectionReturnIntent | null = null;
  if (provider !== null || status !== null) {
    if (isConnectionProvider(provider) && isConnectionResultStatus(status)) {
      intent = { kind: "result", provider, status };
    } else {
      // A mangled result still lands in Connections, without a toast.
      intent = { kind: "open" };
    }
  } else if (settings === "connections") {
    intent = { kind: "open" };
  }
  if (!intent) return null;
  url.searchParams.delete(CONNECTION_RESULT_PARAM);
  url.searchParams.delete(CONNECTION_STATUS_PARAM);
  if (settings === "connections") url.searchParams.delete(SETTINGS_PARAM);
  return { intent, cleaned: `${url.pathname}${url.search}${url.hash}` };
}

export function ConnectionReturn() {
  useEffect(() => {
    const found = readConnectionReturn(window.location.href);
    if (!found) return;
    window.history.replaceState(window.history.state, "", found.cleaned);
    openSettings({ panel: "connections" });
    if (found.intent.kind !== "result") return;
    const { tone, message } = describeConnectionResult(
      found.intent.provider,
      found.intent.status,
    );
    // The app is still cold-loading under the plate when this runs: a
    // success or info toast stays longer to outlast it (errors persist).
    toast[tone](
      message,
      tone === "error"
        ? { id: "connection-result" }
        : { id: "connection-result", duration: 10_000 },
    );
  }, []);
  return null;
}
