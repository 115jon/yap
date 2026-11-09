import { lazy, Suspense, useEffect, useRef, useState } from "react";

import type { User } from "@/lib/types";
import { useChatActions, useChatStore } from "@/stores/chat-store";

const UserProfileModal = lazy(
  () => import("@/components/chat/UserProfileModal"),
);

/**
 * The one full profile (UserProfileModal), driven by the store's
 * `profileUser`: the popover's avatar and every "Profile" menu row set it.
 * The last person stays mounted while the dialog runs its close animation;
 * a different viewer (sign-out, account switch) drops it.
 */
export default function ProfileModalHost() {
  const profileUser = useChatStore((state) => state.profileUser);
  const viewerId = useChatStore((state) => state.user?.id);
  const { setProfileUser } = useChatActions();

  const [renderedProfileUser, setRenderedProfileUser] = useState<User | null>(
    profileUser,
  );
  const profileUserRef = useRef(profileUser);
  const generationRef = useRef(0);
  const previousViewerIdRef = useRef(viewerId);

  useEffect(() => {
    profileUserRef.current = profileUser;
    generationRef.current += 1;
    if (profileUser) {
      // Keep the last payload mounted while the dialog runs its close.
      // eslint-disable-next-line react-hooks-js/set-state-in-effect
      setRenderedProfileUser(profileUser);
    }
  }, [profileUser]);

  useEffect(() => {
    if (previousViewerIdRef.current === viewerId) return;

    previousViewerIdRef.current = viewerId;
    generationRef.current += 1;
    // eslint-disable-next-line react-hooks-js/set-state-in-effect
    setRenderedProfileUser(null);
    if (profileUserRef.current) setProfileUser(null);
  }, [setProfileUser, viewerId]);

  const modalUser = profileUser ?? renderedProfileUser;
  if (!modalUser) return null;
  const generation = generationRef.current;

  return (
    <Suspense fallback={null}>
      <UserProfileModal
        // A new person is a new card (its own fetch, actions and confirm).
        key={modalUser.id}
        open={profileUser != null}
        user={modalUser}
        onClose={() => {
          if (
            profileUserRef.current?.id === modalUser.id &&
            generationRef.current === generation
          ) {
            setProfileUser(null);
          }
        }}
      />
    </Suspense>
  );
}
