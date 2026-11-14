// The web app's side of the deletion grace period: any API answer of 403
// ACCOUNT_PENDING_DELETION (lib/account-deletion.ts on the server) means the
// signed-in account is waiting for deletion. api-client announces it once;
// PendingDeletionGate shows the restore screen instead of the app.

export const ACCOUNT_PENDING_DELETION_CODE = "ACCOUNT_PENDING_DELETION";
export const ACCOUNT_DELETION_GRACE_DAYS = 30;

/** The name a deleted (or pending) account shows to everyone else. */
export const DELETED_USER_DISPLAY_NAME = "Deleted user";
export const ACCOUNT_PENDING_DELETION_EVENT = "yap:account-pending-deletion";

/** What GET /api/account/deletion answers. */
export type AccountDeletionStatus =
  | { status: "active" }
  | {
      status: "pending";
      requested_at: string;
      delete_after: string;
      purging: boolean;
    };

let announced = false;

export function announcePendingDeletion(code: unknown): void {
  if (code !== ACCOUNT_PENDING_DELETION_CODE) return;
  if (typeof window === "undefined") return;
  announced = true;
  window.dispatchEvent(new Event(ACCOUNT_PENDING_DELETION_EVENT));
}

/** Whether an answer said so before anything was listening. */
export function wasPendingDeletionAnnounced(): boolean {
  return announced;
}

/** "December 13, 2026" in the reader's locale. */
export function formatDeletionDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleDateString(undefined, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

/** `delete_after` for a request made at `now`: the grace period later. */
export function deletionDateFrom(now: number): string {
  return new Date(
    now + ACCOUNT_DELETION_GRACE_DAYS * 24 * 60 * 60 * 1000,
  ).toISOString();
}
