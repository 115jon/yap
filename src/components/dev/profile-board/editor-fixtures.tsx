/**
 * Dev-only fixtures for the board editors (mounted for `pb-edit*` and
 * `pb-studio*`):
 *
 *   pb-edit              edit in place in the profile modal, Voice hours
 *                        selected (Loud plan)
 *   pb-edit-picker       the widget picker open (Loud plan)
 *   pb-edit-picker-free  the picker on a free plan: Custom carries the lock
 *   pb-edit-grabbed      Mood picked up with the keyboard, the announcement
 *                        showing in the key bar
 *   pb-edit-conflict     Done met a newer save from another device
 *   pb-studio            the Studio with the Custom widget selected (Loud)
 *   pb-studio-free       the same on a free plan: the upsell plate
 *
 * The board is the owner's (Jon), trimmed from FIXTURE_BOARD_OWN to seven
 * widgets. Saves and uploads hit a fetch stub; the clock is FIXTURE_NOW.
 */

import { useEffect, type ReactNode } from "react";

import {
  ProfileCard,
  ProfileSections,
  resolveProfileCardTheme,
} from "@/components/chat/ProfileCard";
import avatarAda from "@/components/dev/assets/avatar-ada.webp";
import coverGlasshouse from "@/components/dev/assets/cover-glasshouse.webp";
import {
  fail,
  ok,
  useApiStub,
  useFixtureSetup,
  type StubRoute,
} from "@/components/dev/sp-fixtures/api-stub";
import BoardEditor from "@/components/profile-board/BoardEditor";
import {
  FIXTURE_ASSETS,
  FIXTURE_BOARD_OWN,
  FIXTURE_LIVE_GAME,
  FIXTURE_NOW,
  FIXTURE_VIEWER,
} from "@/components/profile-board/fixtures";
import ProfileStudio from "@/components/profile-board/ProfileStudio";
import { draftStorageKey } from "@/components/profile-board/useBoardEditor";
import { Tabs } from "@/components/ui/tabs";
import type { Entitlement } from "@/lib/entitlements";
import type {
  AnyProfileWidget,
  BoardResponse,
  ProfileBoard,
} from "@/lib/profile-board";
import type { User } from "@/lib/types";
import { useChatStore } from "@/stores/chat-store";

import { BoardFixtureMedia } from "./board-fixtures";

const KEEP = [
  "w-nowplay",
  "w-mood",
  "w-countdown",
  "w-voice",
  "w-badges",
  "w-links",
  "w-custom",
];

const WIDGETS = KEEP.map((id) =>
  FIXTURE_BOARD_OWN.board.widgets.find((widget) => widget.id === id)!,
).map((widget) =>
  widget.id === "w-nowplay"
    ? ({ ...widget, size: "M" } as AnyProfileWidget)
    : widget,
);

const BOARD: BoardResponse = {
  ...FIXTURE_BOARD_OWN,
  board: { v: 1, widgets: WIDGETS },
  drafts: [],
  version: 7,
};

/** What "another device" saved: Mood gone, Countdown retitled. */
const THEIR_BOARD: ProfileBoard = {
  v: 1,
  widgets: WIDGETS.filter((widget) => widget.id !== "w-mood").map((widget) =>
    widget.type === "countdown"
      ? ({
          ...widget,
          cfg: { ...widget.cfg, title: "Raid night moved to Saturday" },
        } as AnyProfileWidget)
      : widget,
  ),
};

/** The draft left on this device before the conflict: Links made Medium. */
const MY_DRAFT: ProfileBoard = {
  v: 1,
  widgets: WIDGETS.map((widget) =>
    widget.id === "w-links"
      ? ({ ...widget, size: "M" } as AnyProfileWidget)
      : widget,
  ),
};

const LOUD: Entitlement = {
  tier: "loud",
  source: "grant",
  started_at: "2026-06-01T00:00:00Z",
  expires_at: null,
};
const FREE: Entitlement = {
  tier: "free",
  source: null,
  started_at: null,
  expires_at: null,
};

const JON: User = {
  id: FIXTURE_VIEWER.id,
  username: FIXTURE_VIEWER.username,
  display_name: FIXTURE_VIEWER.display_name,
  status: "online",
  custom_status: "shipping the UI pass",
  bio: "Builds Yap. Mostly in the Lounge. Will trade a Balatro seed for a good raid comp.",
  banner_url: coverGlasshouse,
  avatar_url: avatarAda,
} as User;

let assetSeq = 0;

function routes(conflict: boolean): StubRoute[] {
  return [
    {
      path: /^\/api\/users\/me\/board$/,
      method: "PUT",
      reply: ({ body }) => {
        if (conflict) {
          return {
            status: 409,
            body: {
              error: "Your board changed on another device.",
              code: "BOARD_VERSION_CONFLICT",
              board: THEIR_BOARD,
              version: 9,
            },
            delayMs: 120,
          };
        }
        const sent = body as { board?: ProfileBoard; version?: number } | null;
        if (!sent?.board) return fail(400, "Bad board");
        return ok(
          {
            board: sent.board,
            version: (sent.version ?? 0) + 1,
            drafts: [],
            dropped: [],
            updated_at: new Date(FIXTURE_NOW).toISOString(),
          },
          300,
        );
      },
    },
    {
      path: /^\/api\/users\/me\/board\/assets$/,
      method: "POST",
      reply: () => {
        assetSeq += 1;
        return ok({
          asset: FIXTURE_ASSETS.desk,
          url: `/fixture/${assetSeq}`,
          content_type: "image/webp",
          size: 120_000,
        });
      },
    },
    {
      path: /^\/api\/users\/me\/board$/,
      method: "GET",
      reply: () => ok(BOARD),
    },
  ];
}

function surfacePlan(surface: string): Entitlement {
  return surface.endsWith("-free") ? FREE : LOUD;
}

export default function EditorFixtures({ surface }: { surface: string }) {
  const conflict = surface === "pb-edit-conflict";
  useApiStub(routes(conflict));
  const entitlement = surfacePlan(surface);
  const ready = useFixtureSetup(() => {
    const previous = useChatStore.getState();
    useChatStore.setState({
      user: { ...JON, entitlement } as User,
      gameActivityByUserId: {
        ...previous.gameActivityByUserId,
        [FIXTURE_VIEWER.id]: FIXTURE_LIVE_GAME,
      },
    });
    try {
      if (conflict) {
        localStorage.setItem(
          draftStorageKey(FIXTURE_VIEWER.id),
          JSON.stringify({ version: 7, board: MY_DRAFT, at: FIXTURE_NOW }),
        );
      } else {
        localStorage.removeItem(draftStorageKey(FIXTURE_VIEWER.id));
      }
    } catch {
      // No storage: the fixture just starts clean.
    }
    return () =>
      useChatStore.setState({
        user: previous.user,
        gameActivityByUserId: previous.gameActivityByUserId,
      });
  });
  if (!ready) return null;

  if (surface.startsWith("pb-studio")) {
    return (
      <BoardFixtureMedia>
        <div data-testid={surface} className="h-dvh min-h-0 bg-ply-0">
          <ProfileStudio
            fixture={BOARD}
            entitlementOverride={entitlement}
            onClose={() => {}}
            now={FIXTURE_NOW}
            initialSelectedId="w-custom"
          />
        </div>
      </BoardFixtureMedia>
    );
  }

  return (
    <BoardFixtureMedia>
      <EditModal surface={surface}>
        <BoardEditor
          key={surface}
          initial={BOARD}
          entitlement={entitlement}
          owner={{ id: JON.id, name: "Jon", avatarUrl: JON.avatar_url }}
          now={FIXTURE_NOW}
          onExit={() => {}}
          onOpenStudio={() => {}}
          initialSelectedId={surface === "pb-edit" ? "w-voice" : undefined}
          initialGrabbedId={
            surface === "pb-edit-grabbed" ? "w-mood" : undefined
          }
          initialPickerOpen={surface.startsWith("pb-edit-picker")}
        />
        {conflict ? <ClickDone /> : null}
      </EditModal>
    </BoardFixtureMedia>
  );
}

/** The conflict surface presses Done once, so the 409 comes back. */
function ClickDone() {
  useEffect(() => {
    const id = setTimeout(() => {
      document
        .querySelector<HTMLButtonElement>("[data-slot=board-done]")
        ?.click();
    }, 150);
    return () => clearTimeout(id);
  }, []);
  return null;
}

/** pb-board-modal's composition: the card beside the panel, ~1120px. */
function EditModal({
  surface,
  children,
}: {
  surface: string;
  children: ReactNode;
}) {
  return (
    <div className="h-full overflow-y-auto bg-ply-0">
      <div className="flex min-h-full items-start justify-center sm:p-6">
        <div
          data-testid={surface}
          className="@container/modal w-full max-w-[1120px] overflow-hidden rounded-[2px] border border-line-strong bg-ply-3 shadow-modal"
        >
          <div className="grid grid-cols-[376px_minmax(0,1fr)] @max-[720px]/modal:grid-cols-1">
            <ProfileCard
              size="modal"
              user={JON}
              theme={resolveProfileCardTheme(JON)}
              status="online"
              playAnimation={false}
              renderFrame={false}
              className="border-r border-line @max-[720px]/modal:border-r-0 @max-[720px]/modal:border-b"
            >
              <ProfileSections
                bio={JON.bio}
                memberSince={{ label: "Member since", value: "26 Jun 2025" }}
              />
            </ProfileCard>
            <div className="@container/panel flex min-w-0 flex-col bg-ply-3">
              <div className="sticky top-0 z-10 flex h-14 items-end gap-3 border-b border-line bg-ply-3 px-7 @max-[540px]/panel:h-12 @max-[540px]/panel:px-4">
                <Tabs
                  label="Jon's profile"
                  items={[
                    { value: "board", label: "Board" },
                    { value: "activity", label: "Activity" },
                    { value: "wishlist", label: "Wishlist" },
                  ]}
                  value="board"
                  onValueChange={() => {}}
                  scrollable
                  className="min-w-0 flex-1 self-stretch border-b-0 [&>[role=tab]]:min-h-full"
                />
              </div>
              <div className="px-7 pt-5 pb-7 @max-[540px]/panel:px-4 @max-[540px]/panel:pt-4 @max-[540px]/panel:pb-6">
                {children}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
