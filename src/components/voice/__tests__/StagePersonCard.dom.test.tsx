// @vitest-environment jsdom

import { act, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { StagePersonCard } from "@/components/voice/StagePersonCard";
import type { AmbientPicture } from "@/lib/ambient-color";
import { useChatStore } from "@/stores/chat-store";

const readAmbientPicture =
  vi.fn<(url: string) => Promise<AmbientPicture | null>>();

vi.mock("@/lib/ambient-color", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/ambient-color")>()),
  peekAmbientPicture: () => undefined,
  readAmbientPicture: (url: string) => readAmbientPicture(url),
}));

function card(el: HTMLElement) {
  return el.querySelector<HTMLElement>("[data-stage-person]")!;
}

describe("StagePersonCard", () => {
  beforeEach(() => {
    readAmbientPicture.mockReset();
    readAmbientPicture.mockReturnValue(new Promise(() => {}));
  });
  afterEach(() => {
    useChatStore.setState({ speakingUsers: {} });
  });

  it("renders the full-size picture, not a thumbnail rendition", () => {
    const { container, rerender } = render(
      <StagePersonCard
        userId="u-ada"
        name="Ada"
        avatar="/api/avatars/u-ada/up-1.gif"
      />,
    );
    const img = container.querySelector("[data-stage-face] img")!;
    expect(img.getAttribute("src")).toContain("/api/avatars/u-ada/up-1.gif");
    expect(img.getAttribute("alt")).toBe("Ada");
    expect(img.getAttribute("loading")).toBeNull();

    rerender(
      <StagePersonCard
        userId="u-ada"
        name="Ada"
        avatar="https://cdn.discordapp.com/avatars/1/a.png?size=64"
      />,
    );
    expect(
      container.querySelector("[data-stage-face] img")!.getAttribute("src"),
    ).toBe("https://cdn.discordapp.com/avatars/1/a.png?size=512");
  });

  it("glows in the picture's colour once it is measured", async () => {
    readAmbientPicture.mockResolvedValue({
      wash: null,
      color: "hsl(330 90% 45%)",
    });
    const { container } = render(
      <StagePersonCard userId="u-ada" name="Ada" avatar="/a.png" />,
    );
    expect(card(container).dataset.ambient).toBe("pending");
    await waitFor(() => expect(card(container).dataset.ambient).toBe("avatar"));
    expect(card(container).style.getPropertyValue("--ambient")).toBe(
      "hsl(330 90% 45%)",
    );
  });

  it("falls back to the accent when the colour cannot be read", async () => {
    readAmbientPicture.mockResolvedValue({ wash: null, color: null });
    const { container } = render(
      <StagePersonCard userId="u-ada" name="Ada" avatar="/a.png" />,
    );
    await waitFor(() => expect(card(container).dataset.ambient).toBe("accent"));
    expect(card(container).style.getPropertyValue("--ambient")).toBe(
      "var(--accent-ink)",
    );
    // The picture itself still shows.
    expect(container.querySelector("[data-stage-face] img")).not.toBeNull();
  });

  it("falls back to the accent when the picture cannot be measured at all", async () => {
    readAmbientPicture.mockResolvedValue(null);
    const { container } = render(
      <StagePersonCard userId="u-ada" name="Ada" avatar="/b.png" />,
    );
    await waitFor(() => expect(card(container).dataset.ambient).toBe("accent"));
    expect(container.querySelector("[data-stage-wash]")).toBeNull();
  });

  it("uses the profile tint and the initial without a picture", () => {
    const { container } = render(
      <StagePersonCard userId="u-grace" name="Grace" avatar={null} />,
    );
    expect(card(container).dataset.ambient).toBe("tint");
    expect(card(container).style.getPropertyValue("--ambient")).not.toBe("");
    expect(container.querySelector("[data-stage-face] img")).toBeNull();
    expect(
      container.querySelector('[data-stage-face] [role="img"]')!.textContent,
    ).toBe("G");
    expect(readAmbientPicture).not.toHaveBeenCalled();
  });

  it("shows the speaking ring while they speak", () => {
    const { container } = render(
      <StagePersonCard userId="u-ada" name="Ada" avatar={null} />,
    );
    const face = container.querySelector<HTMLElement>("[data-stage-face]")!;
    expect(face.querySelector("[data-stage-ring]")).not.toBeNull();
    expect(face.hasAttribute("data-speaking")).toBe(false);

    act(() => useChatStore.setState({ speakingUsers: { "u-ada": true } }));
    expect(face.hasAttribute("data-speaking")).toBe(true);

    act(() => useChatStore.setState({ speakingUsers: {} }));
    expect(face.hasAttribute("data-speaking")).toBe(false);
  });

  it("takes a forced speaking state over the store", () => {
    const { container } = render(
      <StagePersonCard userId="u-ada" name="Ada" avatar={null} speaking />,
    );
    expect(
      container
        .querySelector("[data-stage-face]")!
        .hasAttribute("data-speaking"),
    ).toBe(true);
  });
});
