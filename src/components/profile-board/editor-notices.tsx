/**
 * The editors' notices, one line each above the board: a restored local
 * draft, a version conflict with another device, and a failed save with
 * its retry. Plus the two questions: discard unsaved changes, and finish.
 */

import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Icon, type IconName } from "@/components/ui/icon";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogFooter,
  ResponsiveDialogHeader,
  ResponsiveDialogTitle,
} from "@/components/ui/responsive-dialog";
import type { BoardResponse } from "@/lib/profile-board";
import { cn } from "@/lib/utils";

import type { BoardEditorStore } from "./useBoardEditor";

function Notice({
  icon,
  tone,
  title,
  children,
  actions,
  slot,
  role = "status",
}: {
  icon: IconName;
  tone: "quiet" | "warning" | "danger";
  title: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
  slot: string;
  role?: "status" | "alert";
}) {
  return (
    <div
      data-slot={slot}
      role={role}
      className={cn(
        "flex flex-wrap items-center gap-x-3 gap-y-2 rounded-[2px] border px-3 py-2.5",
        tone === "quiet" && "border-line-strong bg-tint-1",
        tone === "warning" && "border-warning-ink/50 bg-warning-tint",
        tone === "danger" && "border-danger-ink/50 bg-danger-tint",
      )}
    >
      <Icon
        name={icon}
        size="sm"
        className={cn(
          "shrink-0",
          tone === "quiet" && "text-ink-2",
          tone === "warning" && "text-warning-ink",
          tone === "danger" && "text-danger-ink",
        )}
      />
      <p className="min-w-0 flex-1 text-ui-sm text-pretty text-ink-2">
        <span className="font-semibold text-ink-1">{title}</span>
        {children ? <> {children}</> : null}
      </p>
      {actions ? (
        <div className="flex shrink-0 items-center gap-2">{actions}</div>
      ) : null}
    </div>
  );
}

/** Whichever notice the editor's state calls for (at most one). */
export function EditorNotices({
  editor,
  onAfterKeepMine,
}: {
  editor: BoardEditorStore;
  /** Keep mine saved (edit in place finishes if Done started the save). */
  onAfterKeepMine?(saved: BoardResponse | null): void;
}) {
  if (editor.conflict) {
    return (
      <Notice
        slot="board-conflict"
        role="alert"
        icon="warning"
        tone="warning"
        title="Your board changed on another device."
        actions={
          <>
            <Button variant="ghost" size="sm" onClick={editor.takeTheirs}>
              Use theirs
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={async () => {
                const saved = await editor.keepMine();
                onAfterKeepMine?.(saved);
              }}
            >
              Keep mine
            </Button>
          </>
        }
      >
        Use the saved copy from there, or keep yours and save it over that one.
      </Notice>
    );
  }
  if (editor.saveError) {
    return (
      <Notice
        slot="board-save-error"
        role="alert"
        icon="warning"
        tone="danger"
        title="Not saved."
        actions={
          <Button
            variant="secondary"
            size="sm"
            loading={editor.saveStatus === "saving"}
            onClick={() => void editor.save()}
          >
            Try again
          </Button>
        }
      >
        {editor.saveError}
      </Notice>
    );
  }
  if (editor.restored) {
    return (
      <Notice
        slot="board-restored"
        icon="reset"
        tone="quiet"
        title="Restored your unsaved changes."
        actions={
          <>
            <Button variant="ghost" size="sm" onClick={editor.dropRestored}>
              Drop them
            </Button>
            <Button variant="ghost" size="sm" onClick={editor.dismissRestored}>
              Keep
            </Button>
          </>
        }
      >
        They were still on this device from last time.
      </Notice>
    );
  }
  return null;
}

/** Discard: asked only when there is something to lose. */
export function DiscardDialog({
  open,
  onOpenChange,
  onDiscard,
  title = "Discard your changes?",
  confirmLabel = "Discard changes",
}: {
  open: boolean;
  onOpenChange(open: boolean): void;
  onDiscard(): void;
  title?: string;
  confirmLabel?: string;
}) {
  return (
    <ConfirmDialog
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      description="Your board goes back to how it was last saved. This can't be undone."
      confirmLabel={confirmLabel}
      cancelLabel="Keep editing"
      onConfirm={onDiscard}
    />
  );
}

/** Esc with nothing picked up: finish editing? */
export function FinishDialog({
  open,
  onOpenChange,
  dirty,
  onFinish,
}: {
  open: boolean;
  onOpenChange(open: boolean): void;
  dirty: boolean;
  onFinish(): void;
}) {
  return (
    <ResponsiveDialog open={open} onOpenChange={onOpenChange}>
      <ResponsiveDialogContent size="sm">
        <ResponsiveDialogHeader>
          <ResponsiveDialogTitle>Done editing?</ResponsiveDialogTitle>
          <ResponsiveDialogDescription>
            {dirty
              ? "Your changes are saved to your board when you finish."
              : "Nothing has changed since your last save."}
          </ResponsiveDialogDescription>
        </ResponsiveDialogHeader>
        <ResponsiveDialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Keep editing
          </Button>
          <Button
            variant="primary"
            onClick={() => {
              onOpenChange(false);
              onFinish();
            }}
          >
            {dirty ? "Save and finish" : "Done"}
          </Button>
        </ResponsiveDialogFooter>
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}
