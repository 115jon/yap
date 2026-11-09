// @vitest-environment jsdom

// The Deadlock plate in every state and size, from the fixture digest (the
// real reduction over API-shaped rows) and presence: what each viewer sees,
// the live variants, the owner-only states and the credit.

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/components/chat/settings/settings-links", () => ({
  openSettings: vi.fn(),
}));

import { openSettings } from "@/components/chat/settings/settings-links";
import { TooltipProvider } from "@/components/ui/tooltip";
import { DEADLOCK_CREDIT, type DeadlockBoardData } from "@/lib/deadlock";
import type { GameActivity } from "@/lib/gateway-protocol";
import type {
  AnyProfileWidget,
  ProfileWidget,
  WidgetSize,
} from "@/lib/profile-board";
import { useChatStore } from "@/stores/chat-store";

import type { BoardCtx } from "../board-format";
import {
  FIXTURE_DEADLOCK,
  FIXTURE_DIGEST,
  FIXTURE_LIVE,
  FIXTURE_PARTY,
} from "../deadlock-fixtures";
import { FIXTURE_NOW } from "../fixtures";
import { WidgetBody } from "../widgets";

const OWNER = "u-kiwi";

function widget(
  size: WidgetSize,
  cfg: Partial<ProfileWidget<"deadlock">["cfg"]> = {},
): AnyProfileWidget {
  return {
    id: "w-deadlock",
    type: "deadlock",
    size,
    cfg: { at_rest: "career", range: "30d", show_party: true, ...cfg },
  } as AnyProfileWidget;
}

function plate(
  data: DeadlockBoardData | undefined,
  {
    size = "S",
    isOwner = false,
    cfg,
    live,
  }: {
    size?: WidgetSize;
    isOwner?: boolean;
    cfg?: Partial<ProfileWidget<"deadlock">["cfg"]>;
    live?: GameActivity;
  } = {},
) {
  useChatStore.setState({
    gameActivityByUserId: live ? { [OWNER]: live } : {},
  });
  const ctx: BoardCtx = {
    ownerId: OWNER,
    ownerName: "Kiwi",
    isOwner,
    drafts: new Set(),
    now: FIXTURE_NOW,
    at: FIXTURE_NOW,
  };
  return render(
    <TooltipProvider>
      <WidgetBody
        widget={widget(size, cfg)}
        data={data ? { deadlock: data } : {}}
        ctx={ctx}
      />
    </TooltipProvider>,
  );
}

function legendText(container: HTMLElement) {
  return (
    container.querySelector("[data-slot=widget-readout]")?.textContent ?? ""
  );
}

beforeEach(() => {
  useChatStore.setState({
    members: FIXTURE_PARTY.map(
      (user) => ({ user, roles: [] }) as unknown as never,
    ),
    membersByServerId: {},
    relationships: [],
    dmChannels: [],
  });
});

afterEach(() => {
  cleanup();
  useChatStore.setState({ gameActivityByUserId: {}, members: [] });
  vi.clearAllMocks();
});

describe("the digest", () => {
  it("reduces the fixture rows to the mockup's numbers", () => {
    const range = FIXTURE_DIGEST.ranges["30d"];
    expect(range.matches).toBe(112);
    expect(range.pool.map((entry) => entry.matches)).toEqual([
      38, 24, 19, 17, 14,
    ]);
    expect(FIXTURE_DIGEST.last10).toHaveLength(10);
    expect(FIXTURE_DIGEST.rank?.badge).toBe(74);
    expect(FIXTURE_DIGEST.rank_days).toHaveLength(30);
    expect(FIXTURE_DIGEST.all_time.matches).toBe(640);
    expect(FIXTURE_DIGEST.last_match?.items).toHaveLength(6);
    expect(
      new TextEncoder().encode(JSON.stringify(FIXTURE_DIGEST)).length,
    ).toBeLessThan(8 * 1024);
  });
});

describe("career at rest", () => {
  it("S shows the rank, the win rate over the range and the main hero", () => {
    const { container } = plate(FIXTURE_DEADLOCK.ok);
    expect(screen.getByText("Emissary IV")).toBeTruthy();
    expect(screen.getByAltText("Emissary IV rank badge")).toBeTruthy();
    expect(screen.getByText("Haze main")).toBeTruthy();
    expect(legendText(container)).toBe("All modes · 30d");
    // Nothing ticks at rest.
    expect(container.querySelector("time")).toBeNull();
  });

  it("M shows four numbers, the hero pool and the last ten", () => {
    const { container } = plate(FIXTURE_DEADLOCK.ok, { size: "M" });
    for (const label of ["Won", "KDA", "Games", "Hours"]) {
      expect(screen.getByText(label)).toBeTruthy();
    }
    expect(
      screen.getByRole("list", { name: "Hero pool, 30 days" }),
    ).toBeTruthy();
    const last = screen.getByRole("list", {
      name: "Last 10 matches, newest first",
    });
    expect(last.querySelectorAll("li")).toHaveLength(10);
    expect(container.textContent).toContain("6–4");
  });

  it("L shows the pool, the strip and the rank history with a table", () => {
    const { container } = plate(FIXTURE_DEADLOCK.ok, { size: "L" });
    expect(legendText(container)).toBe("All modes · 30 days");
    expect(
      screen.getByRole("list", { name: "Hero pool, 30 days" }),
    ).toBeTruthy();
    expect(screen.getByText(/Rank at the end of each day/)).toBeTruthy();
    expect(container.querySelectorAll("table tbody tr")).toHaveLength(30);
  });

  it("follows the owner's range", () => {
    const { container } = plate(FIXTURE_DEADLOCK.ok, { cfg: { range: "7d" } });
    expect(legendText(container)).toBe("All modes · 7d");
  });

  it("shows the last match when the owner picked it (S and M)", () => {
    const small = plate(FIXTURE_DEADLOCK.ok, {
      cfg: { at_rest: "last_match" },
    });
    expect(small.container.textContent).toContain("Last match · Haze");
    expect(screen.getByText("Win")).toBeTruthy();
    cleanup();
    plate(FIXTURE_DEADLOCK.ok, { size: "M", cfg: { at_rest: "last_match" } });
    expect(screen.getByText("Final build")).toBeTruthy();
    expect(
      screen.getAllByRole("img", { name: /Fleetfoot|Unstoppable/ }),
    ).toHaveLength(2);
    expect(screen.getByLabelText("9 kills, 3 deaths, 9 assists")).toBeTruthy();
  });

  it("credits the source on every plate", () => {
    const { container } = plate(FIXTURE_DEADLOCK.ok);
    expect(
      container.querySelector(`[title="${DEADLOCK_CREDIT}"]`),
    ).toBeTruthy();
    expect(screen.getByText(DEADLOCK_CREDIT)).toBeTruthy();
  });
});

describe("live", () => {
  it.each(["S", "M", "L"] as const)(
    "%s in a match: hero, mode, match clock and the party",
    (size) => {
      const { container } = plate(FIXTURE_DEADLOCK.ok, {
        size,
        live: FIXTURE_LIVE.match,
      });
      expect(legendText(container)).toBe("In match");
      expect(
        container
          .querySelector("[data-slot=deadlock-live]")
          ?.getAttribute("data-phase"),
      ).toBe("match");
      expect(container.textContent).toContain("23:14");
      expect(container.textContent).toContain("With Mo and Ash");
    },
  );

  it("hides the party when the owner turned it off", () => {
    const { container } = plate(FIXTURE_DEADLOCK.ok, {
      live: FIXTURE_LIVE.match,
      cfg: { show_party: false },
    });
    expect(container.textContent).not.toContain("Mo and Ash");
  });

  it("names only party members the viewer knows", () => {
    useChatStore.setState({ members: [] });
    const { container } = plate(FIXTURE_DEADLOCK.ok, {
      live: FIXTURE_LIVE.match,
    });
    expect(container.textContent).not.toContain("With ");
  });

  it("in queue: amber, the queue clock and the party", () => {
    const { container } = plate(FIXTURE_DEADLOCK.ok, {
      live: FIXTURE_LIVE.queue,
    });
    expect(legendText(container)).toBe("In queue");
    expect(screen.getByText("Looking for a match")).toBeTruthy();
    expect(container.textContent).toContain("2:41");
    expect(container.textContent).toContain("Party of 2 with Mo");
  });

  it("in the Hideout: the last result", () => {
    const { container } = plate(FIXTURE_DEADLOCK.ok, {
      live: FIXTURE_LIVE.hideout,
    });
    expect(legendText(container)).toBe("In game");
    expect(screen.getByText("In the Hideout")).toBeTruthy();
    expect(container.textContent).toContain("Last match");
  });

  it.each(["S", "M", "L"] as const)(
    "%s without the log: Playing Deadlock with its uptime",
    (size) => {
      const { container } = plate(FIXTURE_DEADLOCK.ok, {
        size,
        live: FIXTURE_LIVE.playing,
      });
      expect(legendText(container)).toBe("Playing");
      expect(container.textContent).toContain("Playing Deadlock");
      expect(container.textContent).toContain("1:12:00");
    },
  );

  it("returns to career when the activity is another game", () => {
    const { container } = plate(FIXTURE_DEADLOCK.ok, {
      live: { ...FIXTURE_LIVE.playing, name: "Balatro", app_id: undefined },
    });
    expect(legendText(container)).toBe("All modes · 30d");
  });
});

describe("states", () => {
  it("not linked: the owner gets Link Steam, which opens Connections", () => {
    plate(FIXTURE_DEADLOCK.notLinked, { isOwner: true });
    expect(screen.getByText("Show your Deadlock stats")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Link Steam/ }));
    expect(openSettings).toHaveBeenCalledWith({ panel: "connections" });
  });

  it.each(["notLinked", "empty", "protected"] as const)(
    "%s renders nothing for anyone else",
    (state) => {
      const { container } = plate(FIXTURE_DEADLOCK[state]);
      expect(container.querySelector("[data-slot=widget-plate]")).toBeNull();
    },
  );

  it("no matches: names the linked account to the owner", () => {
    const { container } = plate(FIXTURE_DEADLOCK.empty, { isOwner: true });
    expect(screen.getByText("No matches yet")).toBeTruthy();
    expect(container.textContent).toContain("kiwi_yaps");
    expect(legendText(container)).toBe("Linked");
  });

  it("deleted at source: the owner sees why", () => {
    const { container } = plate(FIXTURE_DEADLOCK.protected, { isOwner: true });
    expect(screen.getByText("Stats turned off at the source")).toBeTruthy();
    expect(legendText(container)).toBe("Only you");
  });

  it.each(["S", "M", "L"] as const)("%s loading: static blocks", (size) => {
    const { container } = plate(FIXTURE_DEADLOCK.pending, { size });
    expect(
      container
        .querySelector("[data-slot=deadlock-loading]")
        ?.getAttribute("aria-busy"),
    ).toBe("true");
    expect(screen.getByText("Loading Deadlock stats")).toBeTruthy();
  });

  it("placement: matches left and last season", () => {
    const { container } = plate(FIXTURE_DEADLOCK.placement);
    expect(screen.getByText("Placement")).toBeTruthy();
    expect(screen.getByText("3 of 8 matches left")).toBeTruthy();
    expect(container.textContent).toContain("Ritualist VI");
    expect(legendText(container)).toBe("Season 2");
  });

  it("stale: the last copy with its age in the legend", () => {
    const { container } = plate(FIXTURE_DEADLOCK.stale);
    expect(legendText(container)).toBe("6h old");
    expect(screen.getByText("Emissary IV")).toBeTruthy();
  });

  it("lapsed: all-time numbers and a dimmed badge", () => {
    const { container } = plate(FIXTURE_DEADLOCK.lapsed);
    expect(legendText(container)).toBe("Played 41d ago");
    expect(container.textContent).toContain("640 matches · 51% won");
    expect(screen.getByAltText("Ritualist badge").className).toContain(
      "opacity-60",
    );
  });

  it("no data yet (a plate just added) reads as loading", () => {
    plate(undefined);
    expect(screen.getByText("Loading Deadlock stats")).toBeTruthy();
  });
});

describe("the picker", () => {
  it("offers Deadlock under Games, saying it needs Steam", async () => {
    const { PickerBody } = await import("../WidgetPicker");
    render(
      <TooltipProvider>
        <PickerBody
          board={{ v: 1, widgets: [] }}
          canCustom
          onAdd={() => {}}
          onClose={() => {}}
        />
      </TooltipProvider>,
    );
    fireEvent.click(screen.getByRole("tab", { name: "Games" }));
    const option = screen
      .getAllByRole("option")
      .find((entry) => entry.getAttribute("data-type") === "deadlock")!;
    expect(option.textContent).toContain("Needs Steam");
    expect(option.textContent).toContain("New");
    expect(
      screen.getAllByRole("option").map((entry) => entry.dataset.type),
    ).toEqual(["deadlock", "now_playing", "top_games"]);
  });
});
