/**
 * Dev-only fixtures for the Rack viewer (mounted for `pb-board*`):
 *
 *   pb-board         Kiwi's full board as a friend sees it: all twelve
 *                    presets and a Custom, Kiwi live in Deadlock
 *   pb-board-own     the same board as its owner on a free plan, with the
 *                    Custom as "Draft · only you" and "Edit board"
 *   pb-board-empty   the owner's empty board (the inviting empty state)
 *   pb-board-states  every widget with nothing to show, as a stranger
 *   pb-board-modal   ProfilePanel beside a ProfileCard at modal size
 *                    (376px card, about 1120px in all; stacks below 720px)
 *
 * The clock is fixed (FIXTURE_NOW). Images are bundled assets mapped in by
 * `BoardFixtureMedia`, never fake server URLs. Guestbook posts, deletes and
 * blocks hit a fetch stub: a note containing "limit" gets the daily-limit
 * 429, so the error path can be seen.
 */

import { useCallback, type ReactNode } from "react";

import {
  ProfileCard,
  ProfileSections,
  resolveProfileCardTheme,
} from "@/components/chat/ProfileCard";
import avatarAda from "@/components/dev/assets/avatar-ada.webp";
import avatarLinus from "@/components/dev/assets/avatar-linus.gif";
import coverCopperline from "@/components/dev/assets/cover-copperline.webp";
import coverGlasshouse from "@/components/dev/assets/cover-glasshouse.webp";
import coverNightdrive from "@/components/dev/assets/cover-nightdrive.webp";
import coverPaperboats from "@/components/dev/assets/cover-paperboats.webp";
import coverSlowfires from "@/components/dev/assets/cover-slowfires.webp";
import coverSmallhours from "@/components/dev/assets/cover-smallhours.webp";
import coverTidelines from "@/components/dev/assets/cover-tidelines.webp";
import {
  fail,
  ok,
  useApiStub,
  useFixtureSetup,
  type StubRoute,
} from "@/components/dev/sp-fixtures/api-stub";
import { BoardMediaProvider } from "@/components/profile-board/board-context";
import {
  FIXTURE_ASSETS,
  FIXTURE_BOARD,
  FIXTURE_BOARD_EMPTY,
  FIXTURE_BOARD_OWN,
  FIXTURE_BOARD_STATES,
  FIXTURE_ICONS,
  FIXTURE_LIVE_GAME,
  FIXTURE_NOW,
  FIXTURE_OWNER,
  FIXTURE_VIEWER,
} from "@/components/profile-board/fixtures";
import { ProfilePanel } from "@/components/profile-board/ProfilePanel";
import type { User } from "@/lib/types";
import { useChatStore } from "@/stores/chat-store";

const ASSET_URLS = {
  [FIXTURE_ASSETS.beach]: coverPaperboats,
  [FIXTURE_ASSETS.cat]: avatarLinus,
  [FIXTURE_ASSETS.desk]: coverSmallhours,
  [FIXTURE_ASSETS.commission]: avatarAda,
} satisfies Record<string, string>;

const ICON_URLS = {
  [FIXTURE_ICONS.deadlock]: coverNightdrive,
  [FIXTURE_ICONS.balatro]: coverGlasshouse,
  [FIXTURE_ICONS.rivals]: coverCopperline,
  [FIXTURE_ICONS.stardew]: coverTidelines,
  [FIXTURE_ICONS.helldivers]: coverSlowfires,
} satisfies Record<string, string>;

/** Resolves the fixtures' asset names and icon paths to bundled images. */
export function BoardFixtureMedia({ children }: { children: ReactNode }) {
  const asset = useCallback(
    (_ownerId: string, name: string) =>
      (ASSET_URLS as Record<string, string>)[name] ?? name,
    [],
  );
  const url = useCallback(
    (path: string) => (ICON_URLS as Record<string, string>)[path] ?? path,
    [],
  );
  return (
    <BoardMediaProvider asset={asset} url={url}>
      {children}
    </BoardMediaProvider>
  );
}

let noteSeq = 0;

function guestbookRoutes(): StubRoute[] {
  return [
    {
      path: /^\/api\/users\/[^/]+\/guestbook$/,
      method: "POST",
      reply: ({ body }) => {
        const text = String((body as { body?: unknown } | null)?.body ?? "");
        if (/limit/i.test(text)) {
          return fail(429, "That's 5 notes today. Try again tomorrow.");
        }
        noteSeq += 1;
        return ok(
          {
            id: `note-new-${noteSeq}`,
            author: FIXTURE_VIEWER,
            body: text,
            created_at: new Date(FIXTURE_NOW).toISOString(),
          },
          400,
        );
      },
    },
    {
      path: /^\/api\/users\/[^/]+\/guestbook\/[^/]+$/,
      method: "DELETE",
      reply: ({ url }) =>
        ok({ id: url.pathname.split("/").pop(), deleted: true }),
    },
    {
      path: /^\/api\/users\/me\/guestbook\/blocks$/,
      method: "POST",
      reply: ({ body }) =>
        ok({
          user_id: (body as { user_id?: string } | null)?.user_id,
          blocked: true,
        }),
    },
  ];
}

const KIWI: User = {
  id: FIXTURE_OWNER.id,
  username: FIXTURE_OWNER.username,
  display_name: FIXTURE_OWNER.name,
  pronouns: "she/they",
  status: "online",
  custom_status: "in ranked, back at 11",
  bio: "Support main who will not stop pinging. Draws cats between queues. Ask me about Balatro seeds.",
  banner_url: coverNightdrive,
  avatar_url: avatarAda,
  badges: FIXTURE_BOARD.data.badges as User["badges"],
} as User;

const JON: User = {
  id: FIXTURE_VIEWER.id,
  username: FIXTURE_VIEWER.username,
  display_name: FIXTURE_VIEWER.display_name,
  status: "online",
  custom_status: "shipping the UI pass",
  bio: "Builds Yap. Mostly in the Lounge.",
  banner_url: coverGlasshouse,
} as User;

function surfaceBoard(surface: string) {
  if (surface === "pb-board-own" || surface === "pb-board-modal-own") {
    return { board: FIXTURE_BOARD_OWN, owner: JON, isOwner: true };
  }
  if (surface === "pb-board-empty") {
    return { board: FIXTURE_BOARD_EMPTY, owner: JON, isOwner: true };
  }
  if (surface === "pb-board-states") {
    return { board: FIXTURE_BOARD_STATES, owner: KIWI, isOwner: false };
  }
  return { board: FIXTURE_BOARD, owner: KIWI, isOwner: false };
}

export default function BoardFixtures({ surface }: { surface: string }) {
  useApiStub(guestbookRoutes());
  const ready = useFixtureSetup(() => {
    const previous = useChatStore.getState().gameActivityByUserId;
    // The empty-states board shows Kiwi idle.
    if (surface === "pb-board-states") return;
    useChatStore.setState({
      gameActivityByUserId: {
        ...previous,
        // Kiwi is live; the owner variant (Jon) shows the same game.
        [FIXTURE_OWNER.id]: FIXTURE_LIVE_GAME,
        [FIXTURE_VIEWER.id]: FIXTURE_LIVE_GAME,
      },
    });
    return () => useChatStore.setState({ gameActivityByUserId: previous });
  });
  if (!ready) return null;

  const { board, owner, isOwner } = surfaceBoard(surface);
  const name = owner.display_name ?? owner.username;
  const modal = surface.startsWith("pb-board-modal");

  return (
    <BoardFixtureMedia>
      <div className="h-full overflow-y-auto bg-ply-0">
        {modal ? (
          <div className="flex min-h-full items-start justify-center sm:p-6">
            <div
              data-testid="pb-board-modal"
              className="@container/modal w-full max-w-[1120px] overflow-hidden rounded-[2px] border border-line-strong bg-ply-3 shadow-modal"
            >
              <div className="grid grid-cols-[376px_minmax(0,1fr)] @max-[720px]/modal:grid-cols-1">
                <ProfileCard
                  size="modal"
                  user={owner}
                  theme={resolveProfileCardTheme(owner)}
                  status="online"
                  playAnimation={false}
                  renderFrame={false}
                  className="border-r border-line @max-[720px]/modal:border-r-0 @max-[720px]/modal:border-b"
                >
                  <ProfileSections
                    bio={owner.bio}
                    memberSince={{
                      label: "Member since",
                      value: "26 Jun 2025",
                    }}
                  />
                </ProfileCard>
                <ProfilePanel
                  userId={owner.id}
                  isOwner={isOwner}
                  displayName={name}
                  avatarUrl={owner.avatar_url}
                  fixtures={{ board }}
                  now={FIXTURE_NOW}
                  onOpenMessage={() => {}}
                />
              </div>
            </div>
          </div>
        ) : (
          <div
            data-testid={surface}
            className="mx-auto w-full max-w-[744px] bg-ply-3 sm:my-6 sm:border sm:border-line-strong"
          >
            <ProfilePanel
              userId={owner.id}
              isOwner={isOwner}
              displayName={name}
              avatarUrl={owner.avatar_url}
              fixtures={{ board }}
              now={FIXTURE_NOW}
              onOpenMessage={() => {}}
            />
          </div>
        )}
      </div>
    </BoardFixtureMedia>
  );
}
