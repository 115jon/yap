import { useAuth } from "@kova/react";
import { useEffect, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import {
  ACCOUNT_PENDING_DELETION_EVENT,
  formatDeletionDate,
  wasPendingDeletionAnnounced,
  type AccountDeletionStatus,
} from "@/lib/account-deletion-events";
import { apiDelete, apiGet } from "@/lib/api-client";
import { describeApiError } from "@/lib/api-errors";
import { hasRecentAuthLogoutIntent } from "@/lib/desktop-auth";
import { signOutToSignIn } from "@/lib/sign-out-to-sign-in";

type Pending = Extract<AccountDeletionStatus, { status: "pending" }>;

/**
 * The signed-in app, unless the account is waiting for deletion: then a
 * screen that offers to restore it. The server refuses such an account
 * everything else (403 ACCOUNT_PENDING_DELETION); api-client announces the
 * first refusal, and this asks /api/account/deletion for the date.
 */
export function PendingDeletionGate({ children }: { children: ReactNode }) {
  const [pending, setPending] = useState<Pending | null>(null);

  useEffect(() => {
    let cancelled = false;
    const check = () => {
      // Asking for deletion signs out; its own 403s are not a sign-in.
      if (hasRecentAuthLogoutIntent()) return;
      void apiGet<AccountDeletionStatus>("/api/account/deletion")
        .then((status) => {
          if (!cancelled && status.status === "pending") setPending(status);
        })
        .catch(() => undefined);
    };
    if (wasPendingDeletionAnnounced()) check();
    window.addEventListener(ACCOUNT_PENDING_DELETION_EVENT, check);
    return () => {
      cancelled = true;
      window.removeEventListener(ACCOUNT_PENDING_DELETION_EVENT, check);
    };
  }, []);

  if (pending) return <PendingDeletionScreen pending={pending} />;
  return <>{children}</>;
}

function PendingDeletionScreen({ pending }: { pending: Pending }) {
  const { signOut } = useAuth();
  const [busy, setBusy] = useState<"restore" | "continue" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const date = formatDeletionDate(pending.delete_after);

  const restore = async () => {
    setBusy("restore");
    setError(null);
    try {
      await apiDelete("/api/account/deletion");
      // A fresh start loads the restored profile, servers and sockets.
      window.location.reload();
    } catch (restoreError) {
      setError(describeApiError(restoreError, "restore your account"));
      setBusy(null);
    }
  };

  const keepDeleting = () => {
    setBusy("continue");
    signOutToSignIn(signOut);
  };

  return (
    <main className="flex min-h-dvh w-full items-center justify-center bg-ply-0 px-4 py-10 font-ui text-ink-1 antialiased">
      <section
        aria-labelledby="pending-deletion-title"
        className="w-full max-w-[440px]"
      >
        <h1
          id="pending-deletion-title"
          className="font-display text-[28px] leading-tight font-semibold text-balance"
        >
          Your account is scheduled for deletion on {date}.
        </h1>
        <p className="mt-3 text-ui-base text-pretty text-ink-2">
          {pending.purging
            ? "Deletion has started and can't be stopped now."
            : "Restore it to keep your profile, friends and settings and rejoin your servers. If you do nothing, it is deleted on that date and messages you sent stay, from “Deleted user”."}
        </p>
        {error ? (
          <p className="mt-3 text-ui-sm text-danger-ink" role="alert">
            {error}
          </p>
        ) : null}
        <div className="mt-6 flex flex-col gap-2 sm:flex-row">
          {pending.purging ? null : (
            <Button
              type="button"
              size="lg"
              loading={busy === "restore"}
              disabled={busy !== null}
              onClick={() => void restore()}
            >
              Restore account
            </Button>
          )}
          <Button
            type="button"
            size="lg"
            variant="secondary"
            loading={busy === "continue"}
            disabled={busy !== null}
            onClick={keepDeleting}
          >
            {pending.purging ? "Sign out" : "Continue deletion"}
          </Button>
        </div>
      </section>
    </main>
  );
}
