// @vitest-environment jsdom

import { QueryClient } from "@tanstack/react-query";
import { createMemoryHistory, RouterProvider } from "@tanstack/react-router";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it } from "vitest";

import type { AdminSettingsResponse } from "../../shared/admin-api";
import type { AdminTransport } from "../admin-data";
import { AdminNowContext } from "../admin-format";
import { ConsoleProviders } from "../app";
import { FIXTURE_ADMIN_ID, FIXTURE_NOW, fixtureTransport } from "../fixtures";
import { createConsoleRouter } from "../router";
import { ConsoleShell } from "../shell";

// The native handoff card: turning it on and raising the rollout while it
// is on both ask first; lowering it does not.

beforeAll(() => {
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

function settingsTransport(enabled: boolean, rolloutPercent: number) {
  const fixtures = fixtureTransport(null);
  const sent: unknown[] = [];
  let current: AdminSettingsResponse | null = null;
  const transport: AdminTransport = {
    ...fixtures,
    get: async <T,>(path: string): Promise<T> => {
      if (path.startsWith("/api/settings")) {
        if (!current) {
          const base = await fixtures.get<AdminSettingsResponse>(path);
          current = { ...base, nativeHandoff: { enabled, rolloutPercent } };
        }
        return current as T;
      }
      return fixtures.get<T>(path);
    },
    send: async <T,>(_method: string, _path: string, body?: unknown) => {
      sent.push(body);
      const patch = (body as { nativeHandoff: object }).nativeHandoff;
      current = {
        ...current!,
        nativeHandoff: { ...current!.nativeHandoff, ...patch },
      };
      return current as T;
    },
  };
  return { transport, sent };
}

function renderSwitches(transport: AdminTransport) {
  const router = createConsoleRouter(
    createMemoryHistory({ initialEntries: ["/switches"] }),
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
  });
  render(
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
}

async function setRollout(value: string) {
  const user = userEvent.setup();
  const input = await screen.findByLabelText("Rollout");
  await user.clear(input);
  await user.type(input, value);
  await user.click(screen.getByRole("button", { name: "Save rollout" }));
  return user;
}

describe("the native handoff card", () => {
  it("asks before raising the rollout while the handoff is on", async () => {
    const { transport, sent } = settingsTransport(true, 10);
    renderSwitches(transport);
    const user = await setRollout("50");
    expect(await screen.findByText("Raise the rollout to 50%?")).toBeTruthy();
    expect(sent).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Raise rollout" }));
    await waitFor(() =>
      expect(sent).toEqual([{ nativeHandoff: { rolloutPercent: 50 } }]),
    );
  });

  it("lowers the rollout without asking", async () => {
    const { transport, sent } = settingsTransport(true, 10);
    renderSwitches(transport);
    await setRollout("5");
    await waitFor(() =>
      expect(sent).toEqual([{ nativeHandoff: { rolloutPercent: 5 } }]),
    );
    expect(screen.queryByText(/Raise the rollout/)).toBeNull();
  });

  it("raises it without asking while the handoff is off", async () => {
    const { transport, sent } = settingsTransport(false, 0);
    renderSwitches(transport);
    await setRollout("25");
    await waitFor(() =>
      expect(sent).toEqual([{ nativeHandoff: { rolloutPercent: 25 } }]),
    );
  });

  it("asks before turning the handoff on", async () => {
    const { transport, sent } = settingsTransport(false, 10);
    renderSwitches(transport);
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole("switch", { name: "Hand off to the native app" }),
    );
    expect(await screen.findByText("Turn on the native handoff?")).toBeTruthy();
    expect(sent).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Turn on" }));
    await waitFor(() =>
      expect(sent).toEqual([{ nativeHandoff: { enabled: true } }]),
    );
  });
});
