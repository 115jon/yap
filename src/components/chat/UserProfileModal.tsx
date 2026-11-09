import MobileProfileSheet from "@/components/chat/MobileProfileSheet";
import {
  ProfileActionNotices,
  ProfileActionRow,
  ProfileBlockConfirm,
  ProfileCard,
  ProfileRoleChips,
  ProfileSections,
  resolveProfileCardTheme,
  useProfileActions,
  useProfileCall,
  type MutualFriend,
  type MutualServer,
} from "@/components/chat/ProfileCard";
import {
  USER_STATUS_LABELS,
  type UserStatus,
} from "@/components/chat/UserStatusDot";
import { RowError } from "@/components/chat/settings/primitives";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import {
  ResponsiveDialog,
  ResponsiveDialogContent,
  ResponsiveDialogDescription,
  ResponsiveDialogTitle,
} from "@/components/ui/responsive-dialog";
import { useIsMobile } from "@/hooks/useMediaQuery";
import { apiGet } from "@/lib/api-client";
import { describeApiError } from "@/lib/api-errors";
import { getDisplayName } from "@/lib/display-name";
import { resolveProfileReferenceDate } from "@/lib/profile-dates";
import { dispatchOpenProfileEditorEvent } from "@/lib/profile-editor-events";
import type { Role, User } from "@/lib/types";
import { useChatActions, useChatStore } from "@/stores/chat-store";
import { useGameActivity } from "@/hooks/useGameActivity";
import { useCallback, useEffect, useState } from "react";

interface Props {
  open: boolean;
  user: User;
  onClose: () => void;
  /** Open with the block confirmation showing (fixtures). */
  initialBlockConfirm?: boolean;
}

type JoinedMemberRecord = {
  user: User;
  roles?: Role[];
  joined_at?: string | number | null;
};

type ProfileResponse = {
  user: User;
  mutualFriends: { count: number; items: MutualFriend[] };
  mutualServers: { count: number; items: MutualServer[] };
};

/**
 * The full profile: what the popover's avatar and every "Profile" menu row
 * open. A large top-anchored dialog on desktop (the shared ProfileCard at
 * modal size, 640 wide), the phone profile sheet (MobileProfileSheet) on
 * phones. Your own profile has Edit profile in place of the relationship
 * actions.
 */
export default function UserProfileModal(props: Props) {
  const isMobile = useIsMobile();
  if (isMobile) {
    return (
      <MobileProfileSheet
        open={props.open}
        user={props.user}
        onClose={props.onClose}
        onOpenProfileEditor={() => {
          props.onClose();
          dispatchOpenProfileEditorEvent();
        }}
      />
    );
  }
  return <DesktopUserProfileModal {...props} />;
}

/**
 * A top-anchored content dialog: it grows downward, so loading or the block
 * confirm never recentre it; one scroller.
 */
function DesktopUserProfileModal({
  open,
  user,
  onClose,
  initialBlockConfirm = false,
}: Props) {
  const currentUser = useChatStore((state) => state.user);
  const inServer = useChatStore(
    (state) => Boolean(state.activeServerId) && state.activeServerId !== "@me",
  );
  const memberRecord = useChatStore(
    (state) =>
      (state.members as JoinedMemberRecord[]).find(
        (member) => member.user.id === user.id,
      ) ?? null,
  );
  const { loadRelationships } = useChatActions();

  const [profileData, setProfileData] = useState<ProfileResponse | null>(null);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  const isMe = currentUser?.id === user.id;
  const seededUser = (isMe ? currentUser : null) ?? memberRecord?.user ?? user;
  const resolvedUser =
    (profileData?.user.id === user.id ? profileData.user : null) ?? seededUser;
  const gameActivity = useGameActivity(user.id);
  const displayName = getDisplayName(resolvedUser);
  const fallbackName = getDisplayName(user);
  const theme = resolveProfileCardTheme(resolvedUser);
  const status = (resolvedUser.status ?? "offline") as UserStatus;
  const statusLabel = USER_STATUS_LABELS[status] ?? USER_STATUS_LABELS.offline;
  const roles = inServer
    ? (memberRecord?.roles?.filter((role) => !role.is_default) ?? [])
    : [];
  const hasReferenceDate = Boolean(
    memberRecord?.joined_at || resolvedUser.created_at,
  );
  const referenceDate = resolveProfileReferenceDate({
    joinedAt: memberRecord?.joined_at,
    createdAt: resolvedUser.created_at ?? null,
  });

  const actions = useProfileActions({
    userId: resolvedUser.id,
    username: resolvedUser.username,
    displayName,
    isMe,
    onMessageOpened: onClose,
    initialBlockConfirm,
  });
  const { confirmBlock, cancelBlock } = actions;
  const call = useProfileCall({
    userId: resolvedUser.id,
    displayName,
    onStarted: onClose,
  });

  useEffect(() => {
    if (!isMe) void loadRelationships();
  }, [isMe, loadRelationships]);

  useEffect(() => {
    if (isMe) return undefined;

    let cancelled = false;

    void apiGet<ProfileResponse>(`/api/users/${user.id}/profile`)
      .then((data) => {
        if (!cancelled) setProfileData(data);
      })
      .catch((error) => {
        if (cancelled) return;
        setProfileError(
          describeApiError(error, `load ${fallbackName}'s profile`),
        );
      });

    return () => {
      cancelled = true;
    };
  }, [isMe, user.id, fallbackName, attempt]);

  const retry = useCallback(() => {
    setProfileError(null);
    setAttempt((value) => value + 1);
  }, []);

  return (
    <ResponsiveDialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      <ResponsiveDialogContent
        size="lg"
        placement="top"
        initialFocus="surface"
        aria-label={`${displayName}'s profile`}
        // Name effects and decorations play while the profile is open.
        data-decoration-play=""
        onEscapeKeyDown={(event) => {
          // Escape backs out of the block confirmation before the profile.
          if (!confirmBlock) return;
          event.preventDefault();
          cancelBlock();
        }}
        // The profile card is the panel (radius, line, ply-3, shadow); its
        // frame art sits in the dialog box around it. The primitive's close
        // (28/16 at 16/16) sits on the banner.
        className="gap-0"
        dialogClassName="flex flex-col overflow-visible border-0 bg-transparent p-0 shadow-none md:max-w-[640px]"
      >
        <ResponsiveDialogTitle className="sr-only">
          {`${displayName}'s profile`}
        </ResponsiveDialogTitle>
        <ResponsiveDialogDescription className="sr-only">
          {`@${resolvedUser.username}, ${statusLabel}`}
        </ResponsiveDialogDescription>

        <ProfileCard
          size="modal"
          user={resolvedUser}
          theme={theme}
          name={displayName}
          status={status}
          replayEffectOnBannerHover={!isMe}
          surfaceClassName="shadow-modal"
          scrollClassName="md:max-h-[calc(100dvh-max(48px,12vh)-24px-var(--rm-profile-frame-reserve-y,0px))]"
          actions={
            <ProfileActionRow
              controller={actions}
              size="modal"
              loadError={Boolean(profileError)}
              onRetry={retry}
              selfAction={
                <Button
                  onClick={() => {
                    onClose();
                    dispatchOpenProfileEditorEvent();
                  }}
                >
                  <Icon name="edit" size="sm" />
                  Edit profile
                </Button>
              }
              extra={
                <Button variant="secondary" onClick={() => void call()}>
                  <Icon name="call" size="sm" />
                  Call
                </Button>
              }
            />
          }
        >
          <ProfileActionNotices controller={actions} />
          {profileError ? (
            <RowError className="mt-0">{profileError}</RowError>
          ) : null}
          <ProfileSections
            gameActivity={gameActivity}
            bio={resolvedUser.bio}
            connections={resolvedUser.connections}
            bioMarkdown
            roles={roles.length > 0 ? <ProfileRoleChips roles={roles} /> : null}
            mutualServers={isMe ? null : profileData?.mutualServers}
            mutualFriends={isMe ? null : profileData?.mutualFriends}
            memberSince={hasReferenceDate ? referenceDate : null}
          />
          <ProfileBlockConfirm controller={actions} />
        </ProfileCard>
      </ResponsiveDialogContent>
    </ResponsiveDialog>
  );
}
