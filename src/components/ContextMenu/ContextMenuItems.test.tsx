// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { Slider, divider, toActionSheetItems } from "./ContextMenuItems";
import { BOOST_MAX, boostKeyTarget, snapBoost } from "./volume-boost";

describe("Slider (VC-23)", () => {
  it("always shows the thumb and gives coarse pointers a 44px input", () => {
    const onChange = vi.fn();
    const { container } = render(
      <Slider label="Volume" value={50} onChange={onChange} />,
    );
    const thumb = container.querySelector('[data-slot="slider-thumb"]');
    expect(thumb?.className).toContain("size-3");
    expect(thumb?.className).toContain("pointer-coarse:size-5");
    expect(thumb?.className).not.toContain("opacity-0");
    // Without `boost` the range stays 0-200 % with no boost marks.
    expect(thumb).toHaveStyle({ left: "25%" });
    expect(container.querySelector('[data-slot="slider-tick"]')).toBeNull();

    const input = screen.getByRole("slider", { name: "Volume" });
    expect(input).toHaveAttribute("max", "200");
    expect(input).toHaveAttribute("aria-valuetext", "50%");
    expect(input.className).toContain("pointer-coarse:min-h-11");
    fireEvent.change(input, { target: { value: "120" } });
    expect(onChange).toHaveBeenCalledWith(120);
    // Plain sliders leave the keys to the range input.
    fireEvent.keyDown(input, { key: "ArrowRight" });
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});

describe("Slider boost (per-user volume, 0-400 %)", () => {
  it("goes to 400 % with a 100 % tick", () => {
    const { container } = render(
      <Slider label="User volume" value={100} boost onChange={vi.fn()} />,
    );
    const input = screen.getByRole("slider", { name: "User volume" });
    expect(BOOST_MAX).toBe(400);
    expect(input).toHaveAttribute("max", "400");
    expect(input).toHaveAttribute("aria-valuetext", "100%");
    expect(container.querySelector('[data-slot="slider-tick"]')).toHaveStyle({
      left: "25%",
    });
    expect(container.querySelector('[data-slot="slider-thumb"]')).toHaveStyle({
      left: "25%",
    });
    expect(container.querySelector('[data-slot="slider-boost"]')).toBeNull();
  });

  it("lights the range, thumb and readout past 200 %", () => {
    const { container, rerender } = render(
      <Slider label="User volume" value={200} boost onChange={vi.fn()} />,
    );
    expect(container.querySelector('[data-slot="slider-boost"]')).toBeNull();
    expect(
      container.querySelector('[data-slot="slider-value"]')?.className,
    ).not.toContain("text-warning-ink");

    rerender(
      <Slider label="User volume" value={300} boost onChange={vi.fn()} />,
    );
    const hot = container.querySelector('[data-slot="slider-boost"]');
    expect(hot?.className).toContain("bg-warning-ink");
    expect(hot).toHaveStyle({ left: "50%", transform: "scaleX(0.5)" });
    expect(
      container.querySelector('[data-slot="slider-value"]')?.className,
    ).toContain("text-warning-ink");
    expect(
      container.querySelector('[data-slot="slider-thumb"]')?.className,
    ).toContain("bg-warning-ink");
    expect(screen.getByRole("slider")).toHaveAttribute(
      "aria-valuetext",
      "300%",
    );
  });

  it("steps 5 % on arrows and 25 % on PageUp/PageDown, to whole steps", () => {
    expect(boostKeyTarget("ArrowRight", 100)).toBe(105);
    expect(boostKeyTarget("ArrowUp", 103)).toBe(105);
    expect(boostKeyTarget("ArrowLeft", 103)).toBe(100);
    expect(boostKeyTarget("ArrowDown", 100)).toBe(95);
    expect(boostKeyTarget("PageUp", 100)).toBe(125);
    expect(boostKeyTarget("PageUp", 110)).toBe(125);
    expect(boostKeyTarget("PageDown", 110)).toBe(100);
    expect(boostKeyTarget("Home", 250)).toBe(0);
    expect(boostKeyTarget("End", 250)).toBe(400);
    expect(boostKeyTarget("ArrowRight", 398)).toBe(400);
    expect(boostKeyTarget("PageUp", 390)).toBe(400);
    expect(boostKeyTarget("ArrowLeft", 2)).toBe(0);
    expect(boostKeyTarget("Enter", 100)).toBeNull();

    const onChange = vi.fn();
    render(
      <Slider label="User volume" value={103} boost onChange={onChange} />,
    );
    const input = screen.getByRole("slider", { name: "User volume" });
    fireEvent.keyDown(input, { key: "ArrowRight" });
    expect(onChange).toHaveBeenLastCalledWith(105);
    fireEvent.keyDown(input, { key: "PageDown" });
    expect(onChange).toHaveBeenLastCalledWith(100);
    fireEvent.keyDown(input, { key: "End" });
    expect(onChange).toHaveBeenLastCalledWith(400);
  });

  it("snaps a drag to 100 % within 6 px of it", () => {
    // 240 px over 0-400 %: 1 px is 5/3 %, so 6 px is 10 %.
    expect(snapBoost(108, 240)).toBe(100);
    expect(snapBoost(91, 240)).toBe(100);
    expect(snapBoost(111, 240)).toBe(111);
    expect(snapBoost(108, 0)).toBe(108);

    const onChange = vi.fn();
    render(
      <Slider label="User volume" value={100} boost onChange={onChange} />,
    );
    const input = screen.getByRole("slider", { name: "User volume" });
    input.getBoundingClientRect = () => ({ width: 240 }) as DOMRect;
    // Not dragging (assistive tech set the value): no snap.
    fireEvent.change(input, { target: { value: "106" } });
    expect(onChange).toHaveBeenLastCalledWith(106);
    fireEvent.pointerDown(input);
    fireEvent.change(input, { target: { value: "107" } });
    expect(onChange).toHaveBeenLastCalledWith(100);
    fireEvent.change(input, { target: { value: "140" } });
    expect(onChange).toHaveBeenLastCalledWith(140);
    fireEvent.pointerUp(window);
    fireEvent.change(input, { target: { value: "104" } });
    expect(onChange).toHaveBeenLastCalledWith(104);
  });
});

describe("toActionSheetItems", () => {
  it("maps dividers, toggles, sliders and submenus to sheet rows", () => {
    const onMute = vi.fn();
    const items = toActionSheetItems([
      { type: "item", key: "profile", label: "Profile", onSelect: vi.fn() },
      divider("a"),
      {
        type: "item",
        key: "mute",
        label: "Mute",
        checked: true,
        onSelect: onMute,
      },
      {
        type: "slider",
        key: "volume",
        label: "User volume",
        value: 80,
        onChange: vi.fn(),
      },
      {
        type: "submenu",
        key: "apps",
        label: "Apps",
        entries: [
          { type: "item", key: "none", label: "No apps", disabled: true },
        ],
      },
      {
        type: "item",
        key: "disconnect",
        label: "Disconnect",
        danger: true,
      },
    ]);

    expect(items.map((item) => item.label)).toEqual([
      "Profile",
      "Mute",
      "User volume",
      "Apps",
      "Disconnect",
    ]);
    expect(items[0]!.divider).toBe(true);
    expect(items[1]!.rightIcon).toBeTruthy();
    items[1]!.onClick();
    expect(onMute).toHaveBeenCalled();
    // Rows never auto-close; the action decides (as on desktop).
    expect(items.every((item) => item.closeOnClick === false)).toBe(true);
    expect(items[2]!.subtitle).toBe("80%");
    expect(items[2]!.submenu).toBeTruthy();
    expect(items[3]!.submenu).toBeTruthy();
    expect(items[4]!.variant).toBe("danger");
  });
});
