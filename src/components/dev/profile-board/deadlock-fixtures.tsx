/**
 * Dev-only fixtures for the Deadlock widget (mounted for `pb-deadlock*`):
 *
 *   pb-deadlock         every size, at rest and live (in match, in queue,
 *                       in the Hideout, Playing Deadlock without the log)
 *   pb-deadlock-states  not linked, loading, no matches, placement, stale,
 *                       deleted at source and lapsed
 *   pb-deadlock-board   on Kiwi's board in a match (`?offline` for at rest)
 *   pb-deadlock-setup   the owner's settings in the Studio inspector
 *
 * Kiwi's numbers are built by the real reduction from fixture rows. Art is
 * the mockup's, bundled (BoardFixtureMedia maps /api/game-assets paths);
 * presence is seeded per plate, so each live variant is its own owner id.
 */

import type { ReactNode } from "react";

import {
  useFixtureSetup,
  useApiStub,
} from "@/components/dev/sp-fixtures/api-stub";
import { WidgetBody } from "@/components/profile-board/widgets";
import {
  FIXTURE_DEADLOCK,
  FIXTURE_LIVE,
  FIXTURE_PARTY,
} from "@/components/profile-board/deadlock-fixtures";
import {
  FIXTURE_BOARD,
  FIXTURE_BOARD_OWN,
  FIXTURE_NOW,
  FIXTURE_OWNER,
  FIXTURE_VIEWER,
} from "@/components/profile-board/fixtures";
import { ProfilePanel } from "@/components/profile-board/ProfilePanel";
import ProfileStudio from "@/components/profile-board/ProfileStudio";
import type { BoardCtx } from "@/components/profile-board/board-format";
import type { DeadlockBoardData } from "@/lib/deadlock";
import type { GameActivity } from "@/lib/gateway-protocol";
import type {
  AnyProfileWidget,
  BoardResponse,
  ProfileWidget,
  WidgetSize,
} from "@/lib/profile-board";
import type { ChatMember } from "@/lib/chat-reducer";
import { useChatStore } from "@/stores/chat-store";

import { BoardFixtureMedia } from "./board-fixtures";

const LIVE_OWNERS: Record<keyof typeof FIXTURE_LIVE, string> = {
  match: "u-kiwi-match",
  queue: "u-kiwi-queue",
  hideout: "u-kiwi-hideout",
  playing: "u-kiwi-playing",
};

const SIZE_BOX: Record<WidgetSize, string> = {
  S: "h-[168px] w-full max-w-[337px]",
  M: "h-[358px] w-full max-w-[337px]",
  L: "h-[168px] w-full max-w-[688px]",
};

function widget(
  size: WidgetSize,
  cfg: Partial<ProfileWidget<"deadlock">["cfg"]> = {},
): ProfileWidget<"deadlock"> {
  return {
    id: `w-deadlock-${size}`,
    type: "deadlock",
    size,
    cfg: { at_rest: "career", range: "30d", show_party: true, ...cfg },
  };
}

function Spec({
  size,
  title,
  who,
  why,
  data,
  owner = FIXTURE_OWNER.id,
  isOwner = false,
  cfg,
}: {
  size: WidgetSize;
  title: string;
  who: string;
  why?: string;
  data: DeadlockBoardData;
  owner?: string;
  isOwner?: boolean;
  cfg?: Partial<ProfileWidget<"deadlock">["cfg"]>;
}) {
  const ctx: BoardCtx = {
    ownerId: owner,
    ownerName: FIXTURE_OWNER.name,
    isOwner,
    drafts: new Set(),
    now: FIXTURE_NOW,
    at: FIXTURE_NOW,
  };
  return (
    <figure
      data-spec={title}
      className={`m-0 flex flex-col gap-2.5 ${size === "L" ? "w-full max-w-[688px]" : "w-full max-w-[337px]"}`}
    >
      <div className={`${SIZE_BOX[size]} [--plate-ground:var(--ply-3)]`}>
        <WidgetBody
          widget={widget(size, cfg) as AnyProfileWidget}
          data={{ deadlock: data }}
          ctx={ctx}
        />
      </div>
      <figcaption className="flex flex-col gap-1">
        <span className="flex items-baseline gap-2">
          <b className="text-ui-sm font-semibold text-ink-1">{title}</b>
          <span className="silkscreen ml-auto text-ink-3">{who}</span>
        </span>
        {why ? (
          <span className="max-w-[60ch] text-ui-xs text-pretty text-ink-2">
            {why}
          </span>
        ) : null}
      </figcaption>
    </figure>
  );
}

function Tray({
  label,
  hint,
  children,
}: {
  label: string;
  hint: string;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-4">
      <header className="flex items-baseline gap-3 border-b border-line pb-2">
        <b className="font-display text-[18px] font-extrabold tracking-[-0.02em] text-ink-1">
          {label}
        </b>
        <span className="text-ui-sm text-ink-3">{hint}</span>
      </header>
      <div className="flex flex-wrap items-start gap-x-3.5 gap-y-8">
        {children}
      </div>
    </section>
  );
}

function Sizes() {
  const ok = FIXTURE_DEADLOCK.ok;
  return (
    <>
      <Tray label="S" hint="half width, one row · 337 × 168">
        <Spec
          size="S"
          title="Career"
          who="at rest"
          why="Rank badge and win rate over the owner's range."
          data={ok}
        />
        <Spec
          size="S"
          title="In match"
          who="live"
          why="Hero, mode and match time. Party shows Yap friends only."
          data={ok}
          owner={LIVE_OWNERS.match}
        />
        <Spec
          size="S"
          title="In queue"
          who="live"
          data={ok}
          owner={LIVE_OWNERS.queue}
        />
        <Spec
          size="S"
          title="In the Hideout"
          who="live"
          data={ok}
          owner={LIVE_OWNERS.hideout}
        />
        <Spec
          size="S"
          title="Playing Deadlock"
          who="live"
          why="No log (no -condebug): presence only, with its uptime."
          data={ok}
          owner={LIVE_OWNERS.playing}
        />
        <Spec
          size="S"
          title="Last match"
          who="at rest"
          data={ok}
          cfg={{ at_rest: "last_match" }}
        />
      </Tray>
      <Tray label="M" hint="half width, two rows · 337 × 358">
        <Spec
          size="M"
          title="In match"
          who="live"
          data={ok}
          owner={LIVE_OWNERS.match}
        />
        <Spec
          size="M"
          title="Last match"
          who="at rest"
          data={ok}
          cfg={{ at_rest: "last_match" }}
        />
        <Spec size="M" title="Career" who="at rest" data={ok} />
        <Spec
          size="M"
          title="In queue"
          who="live"
          data={ok}
          owner={LIVE_OWNERS.queue}
        />
        <Spec
          size="M"
          title="Playing Deadlock"
          who="live"
          data={ok}
          owner={LIVE_OWNERS.playing}
        />
      </Tray>
      <Tray label="L" hint="full width, one row · 688 × 168">
        <Spec
          size="L"
          title="Career"
          who="at rest"
          why="Hero pool, the last ten and the rank over 30 days with the season reset marked."
          data={ok}
        />
        <Spec
          size="L"
          title="In match"
          who="live"
          data={ok}
          owner={LIVE_OWNERS.match}
        />
        <Spec
          size="L"
          title="Playing Deadlock"
          who="live"
          data={ok}
          owner={LIVE_OWNERS.playing}
        />
      </Tray>
    </>
  );
}

function States() {
  const d = FIXTURE_DEADLOCK;
  return (
    <>
      <Tray label="States" hint="viewers never see an error">
        <Spec
          size="S"
          title="Not linked"
          who="owner only"
          why="Others never receive the plate."
          data={d.notLinked}
          isOwner
        />
        <Spec
          size="S"
          title="Loading"
          who="everyone"
          why="First fetch only, static blocks."
          data={d.pending}
        />
        <Spec
          size="S"
          title="No matches"
          who="owner only"
          data={d.empty}
          isOwner
        />
        <Spec size="S" title="Placement" who="everyone" data={d.placement} />
        <Spec
          size="S"
          title="Stale"
          who="everyone"
          why="The API did not answer: the last good copy, its age in the legend."
          data={d.stale}
        />
        <Spec
          size="S"
          title="Deleted at source"
          who="owner only"
          data={d.protected}
          isOwner
        />
        <Spec
          size="S"
          title="Lapsed player"
          who="everyone"
          why="No match in 30+ days: all-time numbers, badge dimmed."
          data={d.lapsed}
        />
        <Spec
          size="M"
          title="Not linked, M"
          who="owner only"
          why="Lands at M from the picker."
          data={d.notLinked}
          isOwner
        />
        <Spec size="M" title="Placement, M" who="everyone" data={d.placement} />
      </Tray>
      <Tray label="Loading and stale, L" hint="688 × 168">
        <Spec size="L" title="Loading, L" who="everyone" data={d.pending} />
        <Spec size="L" title="Stale, L" who="everyone" data={d.stale} />
      </Tray>
    </>
  );
}

/** Kiwi's board: Deadlock (M) beside a countdown, a mood and a quote. */
const BOARD: BoardResponse = {
  ...FIXTURE_BOARD,
  board: {
    v: 1,
    widgets: [
      { ...widget("M"), id: "w-deadlock" } as AnyProfileWidget,
      ...FIXTURE_BOARD.board.widgets.filter((w) =>
        ["w-countdown", "w-mood", "w-quote"].includes(w.id),
      ),
    ],
  },
  data: { ...FIXTURE_BOARD.data, deadlock: FIXTURE_DEADLOCK.ok },
};

const STUDIO_BOARD: BoardResponse = {
  ...FIXTURE_BOARD_OWN,
  board: {
    v: 1,
    widgets: [
      { ...widget("M"), id: "w-deadlock" } as AnyProfileWidget,
      ...FIXTURE_BOARD_OWN.board.widgets.filter((w) =>
        ["w-countdown", "w-mood"].includes(w.id),
      ),
    ],
  },
  drafts: [],
  data: { ...FIXTURE_BOARD_OWN.data, deadlock: FIXTURE_DEADLOCK.ok },
};

export default function DeadlockFixtures({ surface }: { surface: string }) {
  useApiStub([]);
  const offline =
    typeof window !== "undefined" &&
    new URLSearchParams(window.location.search).has("offline");
  const ready = useFixtureSetup(() => {
    const previous = useChatStore.getState();
    const live: Record<string, GameActivity> = {};
    for (const [phase, owner] of Object.entries(LIVE_OWNERS)) {
      live[owner] = FIXTURE_LIVE[phase as keyof typeof FIXTURE_LIVE];
    }
    if (surface === "pb-deadlock-board" && !offline) {
      live[FIXTURE_OWNER.id] = FIXTURE_LIVE.match;
    }
    const party = FIXTURE_PARTY.map(
      (user) => ({ user, roles: [] }) as unknown as ChatMember,
    );
    useChatStore.setState({
      gameActivityByUserId: { ...previous.gameActivityByUserId, ...live },
      members: [...previous.members, ...party],
    });
    return () =>
      useChatStore.setState({
        gameActivityByUserId: previous.gameActivityByUserId,
        members: previous.members,
      });
  });
  if (!ready) return null;

  if (surface === "pb-deadlock-setup") {
    return (
      <BoardFixtureMedia>
        <div data-testid={surface} className="h-dvh min-h-0 bg-ply-0">
          <ProfileStudio
            fixture={STUDIO_BOARD}
            onClose={() => {}}
            now={FIXTURE_NOW}
            initialSelectedId="w-deadlock"
          />
        </div>
      </BoardFixtureMedia>
    );
  }

  if (surface === "pb-deadlock-board") {
    return (
      <BoardFixtureMedia>
        <div className="h-full overflow-y-auto bg-ply-0">
          <div
            data-testid={surface}
            className="mx-auto w-full max-w-[744px] bg-ply-3 sm:my-6 sm:border sm:border-line-strong"
          >
            <ProfilePanel
              userId={FIXTURE_OWNER.id}
              isOwner={false}
              displayName={FIXTURE_OWNER.name}
              fixtures={{ board: BOARD }}
              now={FIXTURE_NOW}
              onOpenMessage={() => {}}
            />
          </div>
        </div>
      </BoardFixtureMedia>
    );
  }

  return (
    <BoardFixtureMedia>
      <div className="h-full overflow-y-auto bg-ply-0">
        <div
          data-testid={surface}
          className="mx-auto flex w-full max-w-[1180px] flex-col gap-10 bg-ply-3 px-4 py-8 sm:my-6 sm:border sm:border-line-strong sm:px-7"
        >
          {surface === "pb-deadlock-states" ? <States /> : <Sizes />}
          <p className="text-ui-xs text-ink-3">
            Fixture art is Valve's Deadlock artwork, bundled for review. Viewer
            id: {FIXTURE_VIEWER.id}.
          </p>
        </div>
      </div>
    </BoardFixtureMedia>
  );
}
