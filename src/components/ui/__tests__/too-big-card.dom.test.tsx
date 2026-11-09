// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { MB, UPLOAD_LIMITS } from "@/lib/upload-limits";
import { TooBigCard } from "../too-big-card";

describe("TooBigCard", () => {
  it("is an alert named by the whole sentence, with the meter in mono", () => {
    const { container } = render(
      <TooBigCard
        variant="inline"
        situation={{
          kind: "avatar",
          size: Math.round(20.4 * MB),
          limit: UPLOAD_LIMITS.avatar,
          isGif: true,
        }}
        onAction={() => {}}
      />,
    );
    const card = screen.getByRole("alert", {
      name: "Yappy can't carry that one. This GIF is 20.4 MB. Avatars can be up to 8 MB.",
    });
    expect(card).toHaveTextContent("Yappy can't carry that one");
    const meter = container.querySelector('[data-slot="size-meter"]')!;
    expect(meter).toHaveTextContent("8 MB limit");
    expect(meter).toHaveTextContent("20.4 MB");
    expect(
      meter.querySelectorAll('[data-slot="num"].text-num-xs'),
    ).toHaveLength(2);
    expect(meter.querySelector(".text-warning-ink")).toHaveTextContent(
      "20.4 MB",
    );
  });

  it("draws Yappy as decoration that forced colours hide", () => {
    const { container } = render(
      <TooBigCard
        variant="tray"
        situation={{
          kind: "tray-one",
          name: "raid-clip.mp4",
          size: 31 * MB,
          limit: UPLOAD_LIMITS.attachment,
        }}
      />,
    );
    const pose = container.querySelector('[data-slot="yappy-pose"]')!;
    expect(pose).toHaveAttribute("aria-hidden", "true");
    expect(pose).toHaveAttribute("data-pose", "stuffed");
    expect(pose.className).toContain("rm-dither");
    // stuffed is 40 x 34 cells, 2px a cell in the tray.
    expect(pose).toHaveStyle({ width: "80px", height: "68px" });
    // The tray tile has no room for the meter.
    expect(container.querySelector('[data-slot="size-meter"]')).toBeNull();
  });

  it("calls back with the action a key names, in the spec's words", () => {
    const onAction = vi.fn();
    render(
      <TooBigCard
        variant="tray"
        situation={{
          kind: "tray-one",
          name: "raid-clip.mp4",
          size: 31 * MB,
          limit: UPLOAD_LIMITS.attachment,
        }}
        onAction={onAction}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Choose another" }));
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(onAction.mock.calls).toEqual([["choose-another"], ["remove"]]);
  });

  it("shows no keys without a handler (the drop overlay)", () => {
    render(
      <TooBigCard
        variant="overlay"
        situation={{
          kind: "drag-one",
          name: "raid-clip.mp4",
          size: 31 * MB,
          limit: UPLOAD_LIMITS.attachment,
        }}
      />,
    );
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Drop a smaller file, or post a link to it instead.",
    );
  });
});
