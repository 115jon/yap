// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ apiPost: vi.fn() }));

vi.mock("@/lib/api-client", () => ({ apiPost: mocks.apiPost }));

import { issueMediaGrant, verifyMediaCap } from "../media-capability";
import { resetMediaGrantForTests } from "../media-grant";
import { installMediaCapRepair } from "../media-repair";

const SECRET = "test-secret";
let uninstall: () => void = () => undefined;

function image(src: string) {
  const element = document.createElement("img");
  element.setAttribute("src", src);
  document.body.appendChild(element);
  // Stands in for a component's onError fallback (React listens on the element).
  const ownHandler = vi.fn();
  element.addEventListener("error", ownHandler);
  return { element, ownHandler };
}

beforeEach(() => {
  mocks.apiPost.mockReset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
  window.localStorage.setItem("desktop_auth_token", "session-a");
  uninstall = installMediaCapRepair();
});

afterEach(() => {
  uninstall();
  document.body.innerHTML = "";
  window.localStorage.clear();
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  resetMediaGrantForTests();
});

describe("media cap repair", () => {
  it("signs an image that failed before the grant arrived, holding back its fallback", async () => {
    mocks.apiPost.mockResolvedValue(
      issueMediaGrant(SECRET, "user-1", Date.now()),
    );
    const { element, ownHandler } = image("/api/attachments/c1/a1/f.png");

    element.dispatchEvent(new Event("error"));

    await vi.waitFor(() =>
      expect(element.getAttribute("src")).toContain("cap=m1."),
    );
    expect(ownHandler).not.toHaveBeenCalled();
    const url = new URL(element.src);
    expect(
      verifyMediaCap(
        url.searchParams.get("cap") ?? "",
        url,
        SECRET,
        Date.now(),
      ),
    ).toMatchObject({ ok: true, userId: "user-1" });

    // The signed URL failing too reaches the component as usual.
    element.dispatchEvent(new Event("error"));
    expect(ownHandler).toHaveBeenCalledTimes(1);
  });

  it("lets the fallback run when no grant can be had", async () => {
    mocks.apiPost.mockRejectedValue(new Error("503"));
    const { element, ownHandler } = image("/api/emojis/assets/e1");

    element.dispatchEvent(new Event("error"));

    await vi.waitFor(() => expect(ownHandler).toHaveBeenCalledTimes(1));
    expect(element.getAttribute("src")).toBe("/api/emojis/assets/e1");
  });

  it("leaves other images and issued soundboard caps alone", () => {
    for (const src of [
      "https://example.com/a.png",
      "/api/avatars/u1",
      "/api/soundboard/uploads/s1?cap=sbm1.payload.sig",
    ]) {
      const { element, ownHandler } = image(src);
      element.dispatchEvent(new Event("error"));
      expect(ownHandler, src).toHaveBeenCalledTimes(1);
    }
    expect(mocks.apiPost).not.toHaveBeenCalled();
  });
});
