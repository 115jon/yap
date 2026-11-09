// @vitest-environment jsdom

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import {
  fixtureNameStyle,
  NameStylesFixture,
} from "@/components/dev/name-style-fixtures";
import {
  DISPLAY_NAME_EFFECT_OPTIONS,
  type DisplayNameStyle,
} from "@/lib/profile-customization";
import type { User } from "@/lib/types";
import { useChatStore } from "@/stores/chat-store";
import { DECORATION_PLAY_ATTR } from "../ProfileAssetLayer";
import { ProfileDisplayName } from "../ProfileDisplayName";

vi.mock("@/hooks/useCustomEmojiLookup", () => ({
  useCustomEmojiLookup: () => ({}),
}));

const api = vi.hoisted(() => ({
  apiGet: vi.fn((): Promise<unknown> => new Promise(() => {})),
  apiPost: vi.fn(() => Promise.resolve({})),
  apiPut: vi.fn(() => Promise.resolve({})),
  apiDelete: vi.fn(() => Promise.resolve({})),
  apiPatch: vi.fn(() => Promise.resolve({})),
  apiUpload: vi.fn(() => Promise.resolve({})),
}));
vi.mock("@/lib/api-client", () => api);

class ResizeObserverMock {
  observe() {}
  disconnect() {}
  unobserve() {}
}

const EFFECTS = DISPLAY_NAME_EFFECT_OPTIONS.map((o) => o.id);

/**
 * Ancestors (up to `stop`) that would clip a name's paint: overflow hidden
 * or clipped without a clip margin, a `truncate` wrapper, or a mask that
 * is not left unclipped. The name truncates itself; its parents must not.
 */
function clippingAncestors(name: Element, stop: Element | null) {
  const found: string[] = [];
  let node = name.parentElement;
  while (node && node !== stop && node !== document.body) {
    const classes = node.getAttribute("class") ?? "";
    const tokens = classes.split(/\s+/);
    const style = (node as HTMLElement).style;
    const clips =
      tokens.includes("truncate") ||
      tokens.some((t) => /^overflow(-[xy])?-hidden$/.test(t)) ||
      (tokens.includes("overflow-clip") &&
        !tokens.some((t) => t.startsWith("[overflow-clip-margin:"))) ||
      /hidden|clip/.test(style.overflow ?? "") ||
      (tokens.some((t) => t.includes("mask-image")) &&
        !tokens.includes("[mask-clip:no-clip]"));
    if (clips) found.push(`${node.tagName.toLowerCase()}.${tokens.join(".")}`);
    node = node.parentElement;
  }
  return found;
}

function expectStyledName(name: Element, effect: string) {
  expect(name).toHaveClass("rm-name");
  expect(name).toHaveAttribute("data-name-effect", effect);
}

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", ResizeObserverMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe("ProfileDisplayName with every effect", () => {
  it.each(EFFECTS)("%s renders its root, inks and length", (effect) => {
    const { container } = render(
      <ProfileDisplayName
        text="Peggy Quayle"
        displayNameStyle={fixtureNameStyle(effect)}
      />,
    );
    const root = container.querySelector(".rm-name") as HTMLElement;
    expectStyledName(root, effect);
    expect(root.style.getPropertyValue("--nm-n")).toBe("12");
    const yap =
      DISPLAY_NAME_EFFECT_OPTIONS.find((o) => o.id === effect)!.group === "yap";
    expect(root.hasAttribute("data-yap")).toBe(yap);
    if (yap) {
      expect(root.style.getPropertyValue("--nm-a")).toMatch(/^#[0-9A-F]{6}$/);
      expect(root.style.getPropertyValue("--nm-b")).toMatch(/^#[0-9A-F]{6}$/);
    }
    // Still in lists: nothing plays at rest.
    expect(root).not.toHaveAttribute("data-name-play");
    expect(screen.getAllByText(/Peggy Quayle|P/).length).toBeGreaterThan(0);
  });

  it("prints Dither fade through its Bayer tile", () => {
    const { container } = render(
      <ProfileDisplayName
        text="Ada"
        displayNameStyle={fixtureNameStyle("dither")}
      />,
    );
    const root = container.querySelector(".rm-name") as HTMLElement;
    expect(root.style.getPropertyValue("--nm-dither")).toContain(
      "data:image/svg+xml",
    );
    expect(Number(root.style.getPropertyValue("--nm-dr"))).toBeGreaterThan(0);
  });

  it("splits Wave and Glitch names into letters, announced once", () => {
    for (const style of [
      fixtureNameStyle("holo", true),
      fixtureNameStyle("glitch"),
      fixtureNameStyle("solid", true),
    ]) {
      const { container, unmount } = render(
        <ProfileDisplayName text="Ada Q" displayNameStyle={style} />,
      );
      const root = container.querySelector(".rm-name")!;
      expect(root).toHaveAttribute("data-split");
      const letters = root.querySelectorAll(".rm-name-ch");
      expect(letters).toHaveLength(5);
      // A space becomes a no-break space so the inline-block keeps width.
      expect(letters[3]!.textContent).toBe(" ");
      expect(letters[0]!.querySelector(".rm-name-gl")).not.toBeNull();
      expect(root.querySelector(".sr-only")).toHaveTextContent("Ada Q");
      expect(letters[0]!.parentElement).toHaveAttribute("aria-hidden", "true");
      unmount();
    }
  });

  it("does not split a name with emoji; Wave bobs it whole", () => {
    const { container } = render(
      <ProfileDisplayName
        text="Ada 🎉"
        displayNameStyle={fixtureNameStyle("holo", true)}
      />,
    );
    const root = container.querySelector(".rm-name")!;
    expect(root).not.toHaveAttribute("data-split");
    expect(root).toHaveAttribute("data-wave");
    expect(root.querySelector(".rm-name-ch")).toBeNull();
  });

  it("moves a Classic gradient onto the letters when split", () => {
    const { container } = render(
      <ProfileDisplayName
        text="Ada"
        displayNameStyle={fixtureNameStyle("gradient", true)}
      />,
    );
    const root = container.querySelector(".rm-name") as HTMLElement;
    expect(root.style.backgroundImage).toBe("");
    expect(root.style.getPropertyValue("--nm-fill")).toContain(
      "linear-gradient",
    );
    const glyph = root.querySelector(".rm-name-gl") as HTMLElement;
    expect(glyph.style.webkitTextFillColor).toBe("transparent");
  });

  it("plays animated effects only on hover, and always in a profile", () => {
    for (const effect of ["holo", "pixel", "ember"] as const) {
      const { container, unmount } = render(
        <ul>
          <li>
            <button type="button">
              <ProfileDisplayName
                text="Ada"
                displayNameStyle={fixtureNameStyle(effect)}
              />
            </button>
          </li>
        </ul>,
      );
      const root = container.querySelector(".rm-name")!;
      const row = container.querySelector("button")!;
      expect(root).not.toHaveAttribute("data-name-play");
      fireEvent.pointerEnter(row);
      expect(root).toHaveAttribute("data-name-play");
      fireEvent.pointerLeave(row);
      expect(root).not.toHaveAttribute("data-name-play");
      unmount();
    }
    const { container } = render(
      <div {...{ [DECORATION_PLAY_ATTR]: "" }}>
        <ProfileDisplayName
          text="Ada"
          displayNameStyle={fixtureNameStyle("crt", true)}
        />
      </div>,
    );
    expect(container.querySelector(".rm-name")).toHaveAttribute(
      "data-name-play",
    );
  });

  it("never plays a still effect, even in a profile", () => {
    const { container } = render(
      <div {...{ [DECORATION_PLAY_ATTR]: "" }}>
        <ProfileDisplayName
          text="Ada"
          displayNameStyle={fixtureNameStyle("chrome")}
        />
      </div>,
    );
    expect(container.querySelector(".rm-name")).not.toHaveAttribute(
      "data-name-play",
    );
  });

  it("stills every effect under reduced motion", () => {
    vi.stubGlobal(
      "matchMedia",
      (query: string) =>
        ({
          matches: query.includes("reduce"),
          media: query,
          addEventListener() {},
          removeEventListener() {},
        }) as unknown as MediaQueryList,
    );
    const { container } = render(
      <div {...{ [DECORATION_PLAY_ATTR]: "" }}>
        {EFFECTS.map((effect) => (
          <ProfileDisplayName
            key={effect}
            text="Ada"
            displayNameStyle={fixtureNameStyle(effect, true)}
          />
        ))}
      </div>,
    );
    expect(container.querySelectorAll("[data-name-play]")).toHaveLength(0);
  });

  it("marks the ground of a custom light profile card", () => {
    const { container } = render(
      <ProfileDisplayName
        text="Ada"
        displayNameStyle={fixtureNameStyle("holo")}
        backgroundColor="#FAFAFA"
      />,
    );
    expect(container.querySelector(".rm-name")).toHaveAttribute(
      "data-name-ground",
      "light",
    );
  });
});

describe("every effect on every surface, unclipped", () => {
  it("the specimen matrix (member, voice, author, mention, reply, profile)", () => {
    for (const wave of [false, true]) {
      const { container, unmount } = render(
        <NameStylesFixture wave={wave} play={false} />,
      );
      for (const effect of EFFECTS) {
        const section = container.querySelector(
          `[data-testid="ns-${effect}"]`,
        )!;
        const surfaces = section.querySelectorAll("[data-ns-surface]");
        expect(surfaces).toHaveLength(8);
        for (const surface of surfaces) {
          const names = surface.querySelectorAll(".rm-name");
          expect(names).toHaveLength(3);
          for (const name of names) {
            expectStyledName(name, effect);
            expect(name.hasAttribute("data-wave")).toBe(wave);
            expect(
              clippingAncestors(name, surface),
              `${effect} on ${surface.getAttribute("data-ns-surface")}`,
            ).toEqual([]);
          }
        }
      }
      unmount();
    }
    // Two full matrices (every effect × 8 surfaces × 3 names, with and without
    // Wave) take ~5 s on a loaded CI runner, past the 5 s default.
  }, 30_000);

  it("the real mention list", async () => {
    const MessageInput = (await import("../MessageInput")).default;
    const previous = useChatStore.getState().members;
    useChatStore.setState({
      members: EFFECTS.map((effect, i) => ({
        user: {
          id: `u${i}`,
          username: `al${effect}`,
          display_name: `Al ${effect}`,
          avatar_url: null,
          display_name_style: fixtureNameStyle(effect, i % 2 === 0),
        },
        roles: [],
      })) as never,
    });
    try {
      render(
        <TooltipProvider>
          <MessageInput
            channelId="c1"
            channelName="general"
            onSend={() => {}}
            onTyping={() => {}}
          />
        </TooltipProvider>,
      );
      const textarea = screen.getByRole("textbox", {
        name: "Message #general",
      }) as HTMLTextAreaElement;
      act(() => textarea.focus());
      fireEvent.change(textarea, {
        target: { value: "@al", selectionStart: 3, selectionEnd: 3 },
      });
      const listbox = await screen.findByRole("listbox", { name: "Members" });
      const names = listbox.querySelectorAll(".rm-name");
      expect(names.length).toBeGreaterThan(0);
      for (const name of names) {
        const option = name.closest('[role="option"]');
        expect(clippingAncestors(name, option)).toEqual([]);
      }
    } finally {
      useChatStore.setState({ members: previous });
    }
  });

  it("the real profile popover card", async () => {
    const UserProfilePopover = (await import("../UserProfilePopover")).default;
    for (const effect of ["ember", "glitch", "candlelit"] as const) {
      const grace = {
        id: "u-grace",
        username: "grace",
        display_name: "Grace Hopper",
        display_name_style: fixtureNameStyle(effect, true),
      } as User & { display_name_style: DisplayNameStyle };
      useChatStore.setState({
        user: { id: "u-me", username: "me" },
        members: [],
        relationships: [],
        activeServerId: null,
      } as never);
      const anchor = document.createElement("button");
      document.body.appendChild(anchor);
      const { unmount } = render(
        <UserProfilePopover
          userId="u-grace"
          username="grace"
          displayName="Grace Hopper"
          seedUser={grace}
          anchorEl={anchor}
          onClose={() => {}}
        />,
      );
      const name = (await screen.findAllByText("Grace Hopper"))
        .map((n) => n.closest(".rm-name"))
        .find(Boolean)!;
      expectStyledName(name, effect);
      // An open profile plays its name.
      expect(name).toHaveAttribute("data-name-play");
      expect(name.closest(`[${DECORATION_PLAY_ATTR}]`)).not.toBeNull();
      // Nothing clips between the name and the card's 16px padded inset
      // (the card frame's own clip is that far away).
      const inset = name.closest('[class*="px-4"]');
      expect(inset).not.toBeNull();
      expect(clippingAncestors(name, inset), effect).toEqual([]);
      unmount();
      anchor.remove();
    }
  });
});
