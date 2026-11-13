import { AvatarImage } from "@/components/chat/AvatarImage";
import { avatarShape } from "@/components/ui/avatar-shape";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { apiGet, apiPatch, apiPost } from "@/lib/api-client";
import { describeApiError } from "@/lib/api-errors";
import {
  deletionDateFrom,
  formatDeletionDate,
} from "@/lib/account-deletion-events";
import { getDisplayInitial, getDisplayName } from "@/lib/display-name";
import { signOutToSignIn } from "@/lib/sign-out-to-sign-in";
import { getAuthAssetUrl } from "@/lib/platform";
import { safeParse } from "@/lib/schema";
import { signInMethodLabels } from "@/lib/sign-in-methods";
import { DisplayNameSchema, UsernameSchema } from "@/lib/validations";
import { useChatStore } from "@/stores/chat-store";
import { useAuth, useLinkedAccounts, useUser } from "@kova/react";
import { useEffect, useRef, useState } from "react";
import {
  ComingLater,
  ConfirmStrip,
  SettingsCard,
  SettingsRow,
  useConfirm,
  useSettings,
  useSettingsEscape,
  ValueText,
} from "./primitives";
import { dispatchOpenProfileStudioEvent } from "@/lib/profile-studio-events";

const USERNAME_CHECK_DELAY_MS = 400;

type EditableProfileField = "displayName" | "username";
type UsernameCheckState =
  | "idle"
  | "checking"
  | "available"
  | "unavailable"
  | "invalid"
  | "error";

type UsernameCheckResponse = {
  available: boolean;
};

/** Display name and username: a value with Edit, and an inline editor. */
function ProfileFieldRow({
  rowId,
  field,
  label,
  value,
  onSave,
}: {
  rowId: string;
  field: EditableProfileField;
  label: string;
  value: string;
  onSave: (value: string) => Promise<void>;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [usernameCheck, setUsernameCheck] =
    useState<UsernameCheckState>("idle");
  const [checkedUsername, setCheckedUsername] = useState<string | null>(null);
  const editButtonRef = useRef<HTMLButtonElement>(null);
  const wasEditing = useRef(false);
  const savingRef = useRef(false);

  useEffect(() => {
    if (!editing || field !== "username") return undefined;

    const normalized = draft.trim().toLowerCase();
    if (normalized === value.trim().toLowerCase()) {
      setUsernameCheck("available");
      setCheckedUsername(normalized);
      return undefined;
    }
    if (!safeParse(UsernameSchema, draft).success) {
      setUsernameCheck("invalid");
      setCheckedUsername(null);
      return undefined;
    }
    setCheckedUsername(null);

    const controller = new AbortController();
    const timeoutId = window.setTimeout(() => {
      setUsernameCheck("checking");
      void apiGet<UsernameCheckResponse>(
        `/api/check-username?username=${encodeURIComponent(normalized)}`,
        { signal: controller.signal },
      )
        .then((result) => {
          if (!controller.signal.aborted) {
            setCheckedUsername(normalized);
            setUsernameCheck(result.available ? "available" : "unavailable");
          }
        })
        .catch(() => {
          if (!controller.signal.aborted) setUsernameCheck("error");
        });
    }, USERNAME_CHECK_DELAY_MS);

    return () => {
      window.clearTimeout(timeoutId);
      controller.abort();
    };
  }, [draft, editing, field, value]);

  useEffect(() => {
    if (!editing && wasEditing.current) editButtonRef.current?.focus();
    wasEditing.current = editing;
  }, [editing]);

  const handleEdit = () => {
    setDraft(value);
    setError(null);
    setUsernameCheck(field === "username" ? "available" : "idle");
    setCheckedUsername(
      field === "username" ? value.trim().toLowerCase() : null,
    );
    setEditing(true);
  };

  const handleCancel = () => {
    setDraft(value);
    setError(null);
    setUsernameCheck("idle");
    setCheckedUsername(null);
    setEditing(false);
  };

  useSettingsEscape(editing && !saving, handleCancel);

  const handleSave = async () => {
    if (saving || savingRef.current) return;

    const parsed =
      field === "username"
        ? safeParse(UsernameSchema, draft)
        : safeParse(DisplayNameSchema, draft);
    if (!parsed.success) {
      if (field === "username") setUsernameCheck("invalid");
      setError(parsed.issues[0]?.message ?? "Invalid value");
      return;
    }
    const nextValue = parsed.data;

    if (field === "username") {
      if (usernameCheck !== "available" || checkedUsername !== nextValue) {
        return;
      }
    }

    savingRef.current = true;
    setSaving(true);
    setError(null);
    try {
      await onSave(nextValue);
      setEditing(false);
      setUsernameCheck("idle");
    } catch (saveError) {
      const status = (saveError as { status?: number } | null)?.status;
      setError(
        field === "username" && status === 409
          ? "That username is taken. Try another one."
          : describeApiError(
              saveError,
              field === "username"
                ? "save your username"
                : "save your display name",
            ),
      );
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  const usernameMessage =
    field === "username"
      ? usernameCheck === "checking"
        ? "Checking availability…"
        : usernameCheck === "available"
          ? "Username is available"
          : usernameCheck === "unavailable"
            ? "That username is taken. Try another."
            : usernameCheck === "invalid"
              ? "Use 2–32 letters, numbers, dots, hyphens or underscores."
              : usernameCheck === "error"
                ? "Couldn't check availability. Try again."
                : null
      : null;

  return (
    <SettingsRow
      id={rowId}
      layout="inline"
      footer={
        editing ? (
          <div className="mt-3">
            <div className="flex flex-col gap-2 @min-[560px]:flex-row">
              <div className="relative min-w-0 flex-1">
                {field === "username" ? (
                  <span
                    aria-hidden="true"
                    className="pointer-events-none absolute left-4 top-1/2 -translate-y-1/2 text-ui-base text-ink-2"
                  >
                    @
                  </span>
                ) : null}
                <Input
                  autoFocus
                  inputSize="lg"
                  aria-label={label}
                  aria-invalid={
                    error || usernameCheck === "unavailable" ? true : undefined
                  }
                  value={draft}
                  autoComplete={field === "username" ? "username" : "nickname"}
                  spellCheck={false}
                  onChange={(event) => {
                    const nextDraft = event.target.value;
                    setDraft(nextDraft);
                    setError(null);
                    if (field === "username") {
                      const normalized = nextDraft.trim().toLowerCase();
                      setUsernameCheck(
                        normalized === value.trim().toLowerCase()
                          ? "available"
                          : safeParse(UsernameSchema, nextDraft).success
                            ? "checking"
                            : "invalid",
                      );
                      setCheckedUsername(
                        normalized === value.trim().toLowerCase()
                          ? normalized
                          : null,
                      );
                    }
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      void handleSave();
                    }
                  }}
                  className={field === "username" ? "pl-9" : undefined}
                />
              </div>
              <div className="flex shrink-0 justify-end gap-2">
                <Button
                  type="button"
                  variant="ghost"
                  size="lg"
                  onClick={handleCancel}
                  disabled={saving}
                  aria-label={`Cancel ${label.toLowerCase()} edit`}
                >
                  Cancel
                </Button>
                <Button
                  type="button"
                  size="lg"
                  onClick={() => void handleSave()}
                  loading={saving}
                  disabled={
                    field === "username" && usernameCheck !== "available"
                  }
                  aria-label={`Save ${label.toLowerCase()}`}
                >
                  Save
                </Button>
              </div>
            </div>
            <p
              className="mt-2 min-h-4 text-ui-xs text-ink-2"
              role="status"
              aria-live="polite"
            >
              {usernameMessage}
            </p>
            {error ? (
              <p className="mt-1 text-ui-xs text-danger-ink" role="alert">
                {error}
              </p>
            ) : null}
          </div>
        ) : null
      }
    >
      {() =>
        editing ? null : (
          <>
            <ValueText>{field === "username" ? `@${value}` : value}</ValueText>
            <Button
              type="button"
              variant="secondary"
              onClick={handleEdit}
              ref={editButtonRef}
              aria-label={`Edit ${label.toLowerCase()}`}
            >
              Edit
            </Button>
          </>
        )
      }
    </SettingsRow>
  );
}

/** Error copy for a refused deletion request. */
function deletionError(error: unknown): string {
  const failure = error as {
    code?: string;
    message?: string;
    body?: { servers?: Array<{ name?: string }> };
  } | null;
  if (failure?.code === "OWNS_SERVERS") {
    const names = (failure.body?.servers ?? [])
      .map((server) => server.name)
      .filter((name): name is string => !!name);
    return names.length > 0
      ? `You own ${names.join(", ")}. Delete ${names.length === 1 ? "it" : "them"} in server settings first.`
      : (failure.message ?? "Transfer or delete the servers you own first.");
  }
  if (failure?.code === "CONFIRMATION_MISMATCH") {
    return "That isn't your username. Type it exactly as shown.";
  }
  return describeApiError(error, "delete your account");
}

/**
 * Settings → Account → Delete account: a warning, the username typed to
 * confirm, then a request that schedules deletion in 30 days and signs out.
 */
function DeleteAccountRow({ username }: { username: string }) {
  const { signOut } = useAuth();
  const { armed, arm, cancel, triggerRef } = useConfirm();
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The date the warning names, fixed when the warning opens.
  const [deleteOn, setDeleteOn] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const matches =
    typed.trim().replace(/^@/, "").toLowerCase() === username.toLowerCase();

  useEffect(() => {
    if (armed) inputRef.current?.focus({ preventScroll: true });
  }, [armed]);

  const confirm = async () => {
    if (busy) return;
    if (!matches) {
      setError("Type your username exactly as shown to confirm.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await apiPost("/api/account/deletion", { confirm_username: typed });
      // Signed out from here on; every other device already is.
      signOutToSignIn(signOut);
    } catch (requestError) {
      setError(deletionError(requestError));
      setBusy(false);
    }
  };

  return (
    <SettingsRow
      id="delete-account"
      layout="inline"
      footer={
        armed ? (
          <ConfirmStrip
            irreversible
            busy={busy}
            error={error}
            confirmLabel="Delete account"
            onCancel={() => {
              setTyped("");
              setError(null);
              cancel();
            }}
            onConfirm={() => void confirm()}
            consequence={
              <>
                <strong className="font-semibold">
                  Delete your account on{" "}
                  {deleteOn
                    ? formatDeletionDate(deleteOn)
                    : "this day next month"}
                  ?
                </strong>{" "}
                You leave every server and are signed out everywhere now. Then
                your profile, board, friends, settings and uploaded files are
                deleted. Messages you sent stay where they are, from
                &ldquo;Deleted user&rdquo;. Sign in before that date to restore
                your account. Your Kova sign-in is not deleted. Type{" "}
                <strong className="font-semibold">@{username}</strong> to
                confirm.
                <Input
                  ref={inputRef}
                  value={typed}
                  aria-label={`Type ${username} to confirm`}
                  placeholder={username}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(event) => {
                    setTyped(event.target.value);
                    setError(null);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") void confirm();
                  }}
                  className="mt-2 block max-md:min-h-11"
                />
              </>
            }
          />
        ) : null
      }
    >
      {(ids) =>
        armed ? null : (
          <Button
            ref={triggerRef}
            type="button"
            variant="destructive"
            aria-describedby={ids.descId}
            onClick={() => {
              setDeleteOn(deletionDateFrom(Date.now()));
              arm();
            }}
          >
            <Icon name="trash" size="sm" />
            Delete account
          </Button>
        )
      }
    </SettingsRow>
  );
}

export default function AccountPanel() {
  const { user, isLoaded } = useUser();
  const { signOut } = useAuth();
  const linkedAccounts = useLinkedAccounts();
  const { openProfileEditor } = useSettings();
  const chatUser = useChatStore((s) => s.user);
  const loadCurrentUser = useChatStore((s) => s.actions.loadCurrentUser);
  const signOutConfirm = useConfirm();
  const [signingOut, setSigningOut] = useState(false);

  const displayName = getDisplayName({
    display_name: chatUser?.display_name ?? null,
    username: chatUser?.username ?? user?.username ?? "",
  });
  const username = chatUser?.username || user?.username || "Unknown";
  const [savedProfileValues, setSavedProfileValues] = useState<{
    displayName: string | null;
    username: string;
  } | null>(null);
  const profileValues = savedProfileValues
    ? {
        displayName:
          savedProfileValues.displayName ?? savedProfileValues.username,
        username: savedProfileValues.username,
      }
    : { displayName, username };
  const latestProfileValues = useRef<{
    displayName: string | null;
    username: string;
  }>({ displayName, username });
  const [identitySaving, setIdentitySaving] = useState(false);
  const identitySaveChain = useRef(Promise.resolve());

  useEffect(() => {
    if (!savedProfileValues) {
      latestProfileValues.current = { displayName, username };
    }
  }, [displayName, savedProfileValues, username]);

  const email =
    user?.primaryEmailAddress?.emailAddress ??
    (isLoaded === false ? "Loading…" : "No email on file");
  // Kova's linked accounts. If they can't be read, the row stays out rather
  // than guess.
  const signInMethods = linkedAccounts.isLoaded
    ? signInMethodLabels(
        linkedAccounts.accounts.map((account) => account.providerId),
      ).join(", ")
    : "Loading…";
  const avatarUrl = chatUser?.avatar_url
    ? getAuthAssetUrl(chatUser.avatar_url)
    : (user?.imageUrl ?? null);
  const avatarDisplay = chatUser?.avatar_display ?? null;

  const saveProfileField = async (
    field: EditableProfileField,
    value: string,
  ) => {
    const saveOperation = identitySaveChain.current.then(async () => {
      setIdentitySaving(true);
      try {
        await apiPatch("/api/update-profile", {
          [field]: value,
        });
        const current = latestProfileValues.current;
        const nextValues = {
          displayName:
            field === "displayName"
              ? value || null
              : current.displayName || null,
          username: field === "username" ? value : current.username,
        };
        latestProfileValues.current = nextValues;
        setSavedProfileValues(nextValues);
        const refreshed = await loadCurrentUser();
        if (refreshed) setSavedProfileValues(null);
      } finally {
        setIdentitySaving(false);
      }
    });
    identitySaveChain.current = saveOperation.catch(() => undefined);
    return saveOperation;
  };

  const handleSignOut = () => {
    setSigningOut(true);
    signOutToSignIn(signOut);
  };

  return (
    <>
      <SettingsCard id="profile">
        <SettingsRow
          id="profile"
          layout="stack"
          leading={
            <span
              className={cn(
                "relative flex size-11 bg-tint-2",
                avatarShape(avatarUrl && avatarDisplay),
              )}
            >
              {avatarUrl ? (
                <AvatarImage src={avatarUrl} alt="" display={avatarDisplay} />
              ) : (
                <span className="flex size-full items-center justify-center text-ui-base font-semibold text-ink-1">
                  {getDisplayInitial({ name: profileValues.displayName })}
                </span>
              )}
            </span>
          }
        >
          {() => (
            <Button
              type="button"
              onClick={(event) => openProfileEditor(event.currentTarget)}
              disabled={identitySaving}
            >
              <Icon name="edit" size="sm" />
              Edit profile
            </Button>
          )}
        </SettingsRow>
        <SettingsRow id="profile-board" layout="inline">
          {() => (
            <Button
              type="button"
              variant="secondary"
              onClick={() => dispatchOpenProfileStudioEvent()}
            >
              <Icon name="edit" size="sm" />
              Open Studio
            </Button>
          )}
        </SettingsRow>
        <ProfileFieldRow
          rowId="display-name"
          field="displayName"
          label="Display name"
          value={profileValues.displayName}
          onSave={(value) => saveProfileField("displayName", value)}
        />
        <ProfileFieldRow
          rowId="username"
          field="username"
          label="Username"
          value={profileValues.username}
          onSave={(value) => saveProfileField("username", value)}
        />
        <SettingsRow id="server-profiles" layout="inline">
          {() => <ComingLater />}
        </SettingsRow>
      </SettingsCard>

      <SettingsCard id="sign-in">
        <SettingsRow id="email" layout="inline">
          {() => <ValueText>{email}</ValueText>}
        </SettingsRow>
        {linkedAccounts.error ? null : (
          <SettingsRow id="sign-in-method" layout="inline">
            {() => <ValueText>{signInMethods}</ValueText>}
          </SettingsRow>
        )}
        <SettingsRow
          id="sign-out"
          layout="inline"
          footer={
            signOutConfirm.armed ? (
              <ConfirmStrip
                consequence="You'll be signed out on this device, and any call you're in will end."
                confirmLabel="Sign out"
                busy={signingOut}
                onCancel={signOutConfirm.cancel}
                onConfirm={handleSignOut}
              />
            ) : null
          }
        >
          {(ids) => (
            <Button
              ref={signOutConfirm.triggerRef}
              type="button"
              variant="destructive"
              aria-describedby={ids.descId}
              aria-expanded={signOutConfirm.armed}
              onClick={signOutConfirm.arm}
            >
              <Icon name="logout" size="sm" />
              Sign out
            </Button>
          )}
        </SettingsRow>
      </SettingsCard>

      <SettingsCard id="delete-account">
        <DeleteAccountRow username={profileValues.username} />
      </SettingsCard>
    </>
  );
}
