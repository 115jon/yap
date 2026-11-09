// @vitest-environment jsdom

import { QueryClient } from "@tanstack/react-query";
import {
  createBrowserHistory,
  createMemoryHistory,
  RouterProvider,
} from "@tanstack/react-router";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it } from "vitest";

import { AdminNowContext } from "../admin-format";
import type { AdminTransport } from "../admin-data";
import { ConsoleProviders } from "../app";
import { FIXTURE_ADMIN_ID, FIXTURE_NOW, fixtureTransport } from "../fixtures";
import { createConsoleRouter, type ConsoleRouter } from "../router";
import { ConsoleShell } from "../shell";

// The console's routes on memory history with the fixtures' data: the same
// frame, sections and router the Worker serves, without the network.

beforeAll(() => {
  // A wide desktop: min-width queries match, max-width ones do not.
  window.matchMedia = ((query: string) => ({
    matches: query.includes("min-width"),
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
  Element.prototype.scrollIntoView ??= () => {};
});

function renderConsole(
  path: string,
  transport: AdminTransport = fixtureTransport(null),
  { browser = false }: { browser?: boolean } = {},
) {
  if (browser) window.history.replaceState(null, "", path);
  const router: ConsoleRouter = createConsoleRouter(
    browser
      ? createBrowserHistory()
      : createMemoryHistory({ initialEntries: [path] }),
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
  });
  const view = render(
    <ConsoleShell>
      <AdminNowContext.Provider value={FIXTURE_NOW}>
        <ConsoleProviders
          client={client}
          transport={transport}
          session={{ adminId: FIXTURE_ADMIN_ID }}
        >
          <RouterProvider router={router} />
        </ConsoleProviders>
      </AdminNowContext.Provider>
    </ConsoleShell>,
  );
  return { router, client, view };
}

const here = (router: ConsoleRouter) => router.state.location.pathname;
const title = () =>
  screen.findByRole("heading", { level: 1 }).then((h) => h.textContent);
const navButton = (label: string) =>
  screen
    .getByRole("navigation", { name: "Console" })
    .querySelector(`[data-workbench-item="${label}"]`) as HTMLElement;

async function expectAt(router: ConsoleRouter, path: string, heading: string) {
  await waitFor(() => expect(here(router)).toBe(path));
  await waitFor(async () => expect(await title()).toBe(heading));
}

describe("the console's routes", () => {
  it("sends / to /people", async () => {
    const { router } = renderConsole("/");
    await expectAt(router, "/people", "People");
  });

  it("switches section by clicking each nav item", async () => {
    const user = userEvent.setup();
    const { router } = renderConsole("/people");
    await expectAt(router, "/people", "People");
    for (const [id, path, heading] of [
      ["live", "/live", "Live ops"],
      ["insights", "/insights", "Insights"],
      ["switches", "/switches", "Switches"],
      ["audit", "/audit", "Audit log"],
      ["people", "/people", "People"],
    ] as const) {
      await user.click(navButton(id));
      await expectAt(router, path, heading);
    }
  });

  it("switches section from the keyboard: 1-5 and g then a key", async () => {
    const user = userEvent.setup();
    const { router } = renderConsole("/people");
    await expectAt(router, "/people", "People");
    await user.keyboard("2");
    await expectAt(router, "/live", "Live ops");
    await user.keyboard("5");
    await expectAt(router, "/audit", "Audit log");
    await user.keyboard("gs");
    await expectAt(router, "/switches", "Switches");
    await user.keyboard("gi");
    await expectAt(router, "/insights", "Insights");
    await user.keyboard("gp");
    await expectAt(router, "/people", "People");
  });

  it("switches section from the Ctrl+K palette", async () => {
    const user = userEvent.setup();
    const { router } = renderConsole("/people");
    await expectAt(router, "/people", "People");
    await user.keyboard("{Control>}k{/Control}");
    const input = await screen.findByRole("combobox");
    await user.type(input, "Live ops");
    await user.keyboard("{Enter}");
    await expectAt(router, "/live", "Live ops");
  });

  it("opens a deep link to an account, as after a refresh", async () => {
    const { router } = renderConsole("/people/u-ada");
    await expectAt(router, "/people/u-ada", "People");
    expect(
      await screen.findByRole("heading", { name: "Account" }),
    ).toBeTruthy();
  });

  it("stays on the new section when an account was open (the bounce)", async () => {
    // Browser history: the panel's old back-button entry lived there.
    const user = userEvent.setup();
    const { router } = renderConsole("/people/u-ada", undefined, {
      browser: true,
    });
    await expectAt(router, "/people/u-ada", "People");
    await screen.findByRole("heading", { name: "Account" });
    await user.click(navButton("live"));
    await expectAt(router, "/live", "Live ops");
    // The old bug came back 10 ms later, from the panel's history cleanup.
    await act(() => new Promise((resolve) => setTimeout(resolve, 60)));
    expect(here(router)).toBe("/live");
    expect(window.location.pathname).toBe("/live");
    expect(await title()).toBe("Live ops");
  });

  it("goes back and forward through sections and accounts", async () => {
    const user = userEvent.setup();
    const { router } = renderConsole("/people");
    await expectAt(router, "/people", "People");
    await user.click(
      await screen.findByRole("button", { name: /Ada Lovelace/ }),
    );
    await expectAt(router, "/people/u-ada", "People");
    await user.click(navButton("live"));
    await expectAt(router, "/live", "Live ops");
    await user.click(navButton("audit"));
    await expectAt(router, "/audit", "Audit log");

    act(() => router.history.back());
    await expectAt(router, "/live", "Live ops");
    act(() => router.history.back());
    // Opening an account replaced /people (same section), so back lands on it.
    await expectAt(router, "/people/u-ada", "People");
    act(() => router.history.forward());
    await expectAt(router, "/live", "Live ops");
  });

  it("keeps the frame when one section fails to load", async () => {
    const user = userEvent.setup();
    const base = fixtureTransport(null);
    const failing: AdminTransport = {
      ...base,
      get: <T,>(path: string) =>
        path.startsWith("/api/live")
          ? Promise.reject(new Error("boom"))
          : base.get<T>(path),
    };
    const { router } = renderConsole("/live", failing);
    await expectAt(router, "/live", "Live ops");
    expect(await screen.findByText(/couldn.t load/i)).toBeTruthy();
    await user.click(navButton("switches"));
    await expectAt(router, "/switches", "Switches");
  });
});
