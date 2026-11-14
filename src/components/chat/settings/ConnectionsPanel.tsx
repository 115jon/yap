/**
 * Settings › Connections: one card per provider (lib/connections.ts). Not
 * linked, the card offers "Connect <provider>"; linked, it shows the account
 * (avatar, name, when it was verified), Unlink behind an inline confirm, and
 * the "Show on profile" switch.
 *
 * Connecting leaves the app for the provider's sign-in page; the callback
 * lands back on /chat, where ConnectionReturn reopens this panel with a
 * toast. The desktop and phone builds can't carry the browser cookie the
 * flow needs, so they open the web app in the system browser instead.
 */

import { SettingsSwitch } from "@/components/chat/SettingsSwitch";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { LedLabel } from "@/components/ui/led";
import { apiDelete, apiGet, apiPatch, apiPost } from "@/lib/api-client";
import { describeApiError } from "@/lib/api-errors";
import { useClientDateFormatter } from "@/lib/client-date-format";
import {
  CONNECTION_PROVIDER_INFO,
  CONNECTION_PROVIDERS,
  type ConnectionProvider,
  type OwnConnection,
} from "@/lib/connections";
import { buildProxyMediaUrl } from "@/lib/proxy-media-url";
import { useChatStore } from "@/stores/chat-store";
import { useCallVoiceStore } from "@/stores/useCallVoiceStore";
import { useCallback, useEffect, useState } from "react";
import {
  ConfirmStrip,
  RowError,
  SettingsCard,
  SettingsRow,
  useConfirm,
} from "./primitives";
import { isSessionEnded, RecoverButton } from "./session-ended";

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string; sessionEnded: boolean }
  | { status: "ready"; connections: OwnConnection[] };

/** The users/me copy of the profile's connections, after any change. */
function useRefreshProfile() {
  const loadCurrentUser = useChatStore(
    (state) => state.actions.loadCurrentUser,
  );
  return useCallback(() => {
    void loadCurrentUser().catch(() => {});
  }, [loadCurrentUser]);
}

function ProviderTile({ provider }: { provider: ConnectionProvider }) {
  return (
    <span className="flex size-10 shrink-0 items-center justify-center rounded-[2px] bg-tint-2 text-ink-1">
      <Icon name={CONNECTION_PROVIDER_INFO[provider].icon} size="lg" />
    </span>
  );
}

function AccountAvatar({ connection }: { connection: OwnConnection }) {
  const [failed, setFailed] = useState(false);
  if (!connection.avatar_url || failed) {
    return (
      <span
        aria-hidden="true"
        className="flex size-10 shrink-0 items-center justify-center rounded-avatar bg-tint-3 font-display text-[15px] font-extrabold text-ink-1"
      >
        {Array.from(connection.display_name)[0]?.toUpperCase() ?? "?"}
      </span>
    );
  }
  // Remote avatars go through Yap's media proxy, so opening settings doesn't
  // call Steam's CDN from the browser.
  const src = /^https?:\/\//i.test(connection.avatar_url)
    ? buildProxyMediaUrl(connection.avatar_url)
    : connection.avatar_url;
  return (
    <img
      src={src}
      alt=""
      width={40}
      height={40}
      loading="lazy"
      referrerPolicy="no-referrer"
      onError={() => setFailed(true)}
      className="size-10 shrink-0 rounded-avatar bg-tint-3 object-cover"
    />
  );
}

/**
 * True while this user is in a voice channel or a DM call. Connecting
 * leaves the page, which would drop the call, so it asks first.
 */
function useInCall(): boolean {
  const inChannel = useChatStore((state) => {
    const me = state.user?.id;
    if (!me) return false;
    return Object.values(state.voiceChannelStates).some((members) =>
      members.some((member) => member.user_id === me),
    );
  });
  const inDmCall = useCallVoiceStore((state) => state.joined);
  return inChannel || inDmCall;
}

function NotLinkedAccount({ provider }: { provider: ConnectionProvider }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inCall = useInCall();
  const { armed, arm, cancel, triggerRef } = useConfirm();
  const info = CONNECTION_PROVIDER_INFO[provider];

  const start = () => {
    setBusy(true);
    setError(null);
    apiPost<{ url: string }>(`/api/connections/${provider}/start`, {})
      .then(({ url }) => {
        // Leaves the app; the callback brings the person back here.
        window.location.assign(url);
      })
      .catch((cause: unknown) => {
        setError(describeApiError(cause, `start connecting ${info.label}`));
        setBusy(false);
      });
  };

  return (
    <div>
      <div className="flex flex-col gap-3 @min-[560px]:flex-row @min-[560px]:items-center">
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <ProviderTile provider={provider} />
          <div className="min-w-0">
            <p className="text-[15px] leading-5 font-medium text-ink-1">
              {info.label}
            </p>
            <LedLabel state="off" tone="plain">
              Not connected
            </LedLabel>
          </div>
        </div>
        {!armed ? (
          // Full width under the account on a narrow page, trailing on a wide one.
          <Button
            ref={triggerRef}
            type="button"
            variant="secondary"
            size="sm"
            loading={busy}
            className="w-full @min-[560px]:w-auto"
            onClick={inCall ? arm : start}
          >
            <Icon name={info.icon} size="sm" />
            Connect {info.label}
          </Button>
        ) : null}
      </div>
      {armed ? (
        <ConfirmStrip
          consequence={`Connecting opens ${info.label}'s sign-in in this tab, which ends your call.`}
          confirmLabel="Leave call and connect"
          busy={busy}
          error={error}
          onCancel={() => {
            setError(null);
            cancel();
          }}
          onConfirm={start}
        />
      ) : error ? (
        <RowError>{error}</RowError>
      ) : null}
    </div>
  );
}

function LinkedAccount({
  provider,
  connection,
  onUnlinked,
}: {
  provider: ConnectionProvider;
  connection: OwnConnection;
  onUnlinked: () => void;
}) {
  const { armed, arm, cancel, done, triggerRef } = useConfirm();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const formatDate = useClientDateFormatter();
  const label = CONNECTION_PROVIDER_INFO[provider].label;

  return (
    <div>
      <div className="flex items-center gap-3">
        <AccountAvatar connection={connection} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-[15px] leading-5 font-medium text-ink-1">
            {connection.display_name}
          </p>
          <p className="flex min-w-0 flex-wrap items-center gap-x-2 text-ui-sm text-ink-2">
            <LedLabel state="talk" tone="plain">
              Verified
            </LedLabel>
            <span className="truncate">
              {formatDate(connection.verified_at, {
                day: "numeric",
                month: "short",
                year: "numeric",
              })}
            </span>
            {connection.profile_url ? (
              <a
                href={connection.profile_url}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 text-accent-ink underline-offset-[3px] pointer-fine:hover:underline"
              >
                {label} profile
                <Icon name="externalLink" size="xs" />
                <span className="sr-only">(opens in a new tab)</span>
              </a>
            ) : null}
          </p>
        </div>
        {!armed ? (
          <Button
            ref={triggerRef}
            type="button"
            variant="secondary"
            size="sm"
            aria-label={`Unlink ${label} account ${connection.display_name}`}
            onClick={arm}
          >
            Unlink
          </Button>
        ) : null}
      </div>
      {armed ? (
        <ConfirmStrip
          consequence={`${connection.display_name} will be removed from Yap and your profile. You can connect it again any time.`}
          confirmLabel={`Unlink ${label}`}
          busy={busy}
          error={error}
          onCancel={() => {
            setError(null);
            cancel();
          }}
          onConfirm={() => {
            setBusy(true);
            setError(null);
            apiDelete(`/api/connections/${provider}`)
              .then(() => {
                done();
                onUnlinked();
              })
              .catch((cause: unknown) =>
                setError(describeApiError(cause, `unlink ${label}`)),
              )
              .finally(() => setBusy(false));
          }}
        />
      ) : null}
    </div>
  );
}

function ShowOnProfileRow({
  provider,
  connection,
  onChange,
  onSaved,
}: {
  provider: ConnectionProvider;
  connection: OwnConnection;
  /** Optimistic: the new value, or the old one again on failure. */
  onChange: (show: boolean) => void;
  onSaved: () => void;
}) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return (
    <SettingsRow
      id={`${provider}-show-on-profile`}
      layout="inline"
      footer={error ? <RowError>{error}</RowError> : null}
    >
      {(ids) => (
        <SettingsSwitch
          checked={connection.show_on_profile}
          labelledBy={ids.labelId}
          describedBy={ids.descId}
          disabled={saving}
          onChange={() => {
            const next = !connection.show_on_profile;
            setSaving(true);
            setError(null);
            onChange(next);
            apiPatch(`/api/connections/${provider}`, { showOnProfile: next })
              .then(() => onSaved())
              .catch((cause: unknown) => {
                onChange(!next);
                setError(describeApiError(cause, "save this setting"));
              })
              .finally(() => setSaving(false));
          }}
        />
      )}
    </SettingsRow>
  );
}

/** Placeholder at the row's real height while connections load. */
function AccountSkeleton() {
  return (
    <div role="status" className="flex items-center gap-3">
      <span className="sr-only">Loading connections</span>
      <span
        aria-hidden="true"
        className="size-10 shrink-0 rounded-[2px] bg-tint-1"
      />
      <span aria-hidden="true" className="flex flex-1 flex-col gap-1.5">
        <span className="h-4 w-40 max-w-full rounded-[1px] bg-tint-1" />
        <span className="h-3 w-56 max-w-full rounded-[1px] bg-tint-1" />
      </span>
    </div>
  );
}

function ProviderCard({
  provider,
  state,
  onRetry,
  onUpdate,
  onSaved,
  onRemove,
}: {
  provider: ConnectionProvider;
  state: LoadState;
  onRetry: () => void;
  onUpdate: (connection: OwnConnection) => void;
  onSaved: () => void;
  onRemove: (provider: ConnectionProvider) => void;
}) {
  const connection =
    state.status === "ready"
      ? (state.connections.find((item) => item.provider === provider) ?? null)
      : null;

  return (
    <SettingsCard id={`connection-${provider}`}>
      <SettingsRow id={`${provider}-account`} layout="block" hideLabel>
        {() =>
          state.status === "loading" ? (
            <AccountSkeleton />
          ) : state.status === "error" ? (
            <div className="flex flex-col items-start gap-2">
              <RowError className="mt-0 self-stretch">{state.message}</RowError>
              <RecoverButton
                sessionEnded={state.sessionEnded}
                onRetry={onRetry}
              />
            </div>
          ) : connection ? (
            <LinkedAccount
              provider={provider}
              connection={connection}
              onUnlinked={() => onRemove(provider)}
            />
          ) : (
            <NotLinkedAccount provider={provider} />
          )
        }
      </SettingsRow>
      {connection ? (
        <ShowOnProfileRow
          provider={provider}
          connection={connection}
          onChange={(show) =>
            onUpdate({ ...connection, show_on_profile: show })
          }
          onSaved={onSaved}
        />
      ) : null}
    </SettingsCard>
  );
}

async function fetchConnections(): Promise<LoadState> {
  try {
    const data = await apiGet<{ connections: OwnConnection[] }>(
      "/api/connections",
    );
    return { status: "ready", connections: data.connections ?? [] };
  } catch (error) {
    return {
      status: "error",
      message: describeApiError(error, "load your connections"),
      sessionEnded: isSessionEnded(error),
    };
  }
}

export default function ConnectionsPanel() {
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const refreshProfile = useRefreshProfile();

  // Bumped by Try again; the fetch answers in a callback.
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let cancelled = false;
    void fetchConnections().then((next) => {
      if (!cancelled) setState(next);
    });
    return () => {
      cancelled = true;
    };
  }, [attempt]);

  const update = useCallback((connection: OwnConnection) => {
    setState((current) =>
      current.status === "ready"
        ? {
            ...current,
            connections: current.connections.map((item) =>
              item.provider === connection.provider ? connection : item,
            ),
          }
        : current,
    );
  }, []);

  const remove = useCallback(
    (provider: ConnectionProvider) => {
      setState((current) =>
        current.status === "ready"
          ? {
              ...current,
              connections: current.connections.filter(
                (item) => item.provider !== provider,
              ),
            }
          : current,
      );
      refreshProfile();
    },
    [refreshProfile],
  );

  return (
    <>
      {CONNECTION_PROVIDERS.map((provider) => (
        <ProviderCard
          key={provider}
          provider={provider}
          state={state}
          onRetry={() => {
            setState({ status: "loading" });
            setAttempt((value) => value + 1);
          }}
          onUpdate={update}
          onSaved={refreshProfile}
          onRemove={remove}
        />
      ))}
    </>
  );
}
