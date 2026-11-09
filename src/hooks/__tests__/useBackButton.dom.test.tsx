// @vitest-environment jsdom

import { act, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { useBackButton } from "../useBackButton";

function Panel({ onBack }: { onBack: () => boolean }) {
  useBackButton(onBack, true);
  return null;
}

async function settle() {
  // releaseHistoryEntry pops after a 10 ms timer.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 30));
  });
}

describe("useBackButton", () => {
  it("pops its own history entry when the panel closes", async () => {
    window.history.replaceState(null, "", "/people/u1");
    const go = vi.spyOn(window.history, "go");
    const view = render(<Panel onBack={() => true} />);
    expect(window.history.state).toEqual({ backHandlerOpen: true });
    view.unmount();
    await settle();
    expect(go).toHaveBeenCalledWith(-1);
    go.mockRestore();
  });

  it("does not undo a navigation pushed while the panel was open", async () => {
    // The admin console's bounce: a panel open on /people/u1, a click on
    // Live pushes /live, the panel unmounts, and its cleanup used to run
    // history.go(-1), which popped /live and landed back on the person.
    window.history.replaceState(null, "", "/people/u1");
    const go = vi.spyOn(window.history, "go");
    const view = render(<Panel onBack={() => true} />);
    window.history.pushState({ route: "live" }, "", "/live");
    view.unmount();
    await settle();
    expect(go).not.toHaveBeenCalled();
    expect(window.location.pathname).toBe("/live");
    go.mockRestore();
  });
});
