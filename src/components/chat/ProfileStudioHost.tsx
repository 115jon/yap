import { lazy, Suspense, useEffect, useState } from "react";

import {
  Dialog,
  DialogDescription,
  DialogPortal,
  DialogSurface,
  DialogTitle,
} from "@/components/ui/dialog";
import { onOpenProfileStudio } from "@/lib/profile-studio-events";

const ProfileStudio = lazy(
  () => import("@/components/profile-board/ProfileStudio"),
);

/**
 * The profile Studio (the full board editor) as a full-window layer over
 * the app, opened by `dispatchOpenProfileStudioEvent` from edit mode's
 * "Open Studio" and Settings › Profile. The Studio has its own back key and
 * unsaved-changes guard; Escape goes through it.
 */
export default function ProfileStudioHost() {
  const [open, setOpen] = useState(false);

  useEffect(() => onOpenProfileStudio(() => setOpen(true)), []);

  if (!open) return null;
  return (
    <Dialog open onOpenChange={(next) => !next && setOpen(false)}>
      <DialogPortal>
        <DialogSurface
          showCloseButton={false}
          initialFocus="surface"
          // The Studio owns Escape (it asks before dropping unsaved work).
          onEscapeKeyDown={(event) => event.preventDefault()}
          className="fixed inset-0 z-50 flex flex-col bg-ply-1 outline-none"
        >
          <DialogTitle className="sr-only">Profile studio</DialogTitle>
          <DialogDescription className="sr-only">
            Arrange and edit the widgets on your profile board.
          </DialogDescription>
          <Suspense fallback={null}>
            <ProfileStudio onClose={() => setOpen(false)} />
          </Suspense>
        </DialogSurface>
      </DialogPortal>
    </Dialog>
  );
}
