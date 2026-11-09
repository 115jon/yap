// @vitest-environment jsdom

import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ConsoleAppProps } from "../../shared/console-contract";

// Kova and the console bundle are faked; the Worker is a fetch stub. What is
// under test is the machine as Boot runs it: no reloads, the console kept
// through a lost session, a refusal signing Kova out.

const kova = vi.hoisted(() => ({
  isLoaded: true,
  isSignedIn: false,
  getToken: vi.fn(async () => "tok"),
  signOut: vi.fn(async (_url?: string) => {}),
  listeners: new Set<() => void>(),
  set(patch: { isSignedIn?: boolean }) {
    Object.assign(this, patch);
    for (const listener of this.listeners) listener();
  },
}));

vi.mock("@kova/react", async () => {
  const { useEffect, useReducer } = await import("react");
  return {
    useAuth: () => {
      const [, bump] = useReducer((n: number) => n + 1, 0);
      useEffect(() => {
        kova.listeners.add(bump);
        return () => {
          kova.listeners.delete(bump);
        };
      }, []);
      return {
        isLoaded: kova.isLoaded,
        isSignedIn: kova.isSignedIn,
        getToken: kova.getToken,
        signOut: kova.signOut,
      };
    },
    SignIn: () => <form aria-label="Kova sign-in" />,
  };
});

const consoleProps = vi.hoisted(() => ({
  latest: null as ConsoleAppProps | null,
  mounts: 0,
}));

vi.mock("../session", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../session")>();
  const { useEffect } = await import("react");
  function FakeConsole(props: ConsoleAppProps) {
    useEffect(() => {
      consoleProps.latest = props;
    });
    useEffect(() => {
      consoleProps.mounts += 1;
    }, []);
    return (
      <div data-testid="console" data-active={String(props.active)}>
        console of {props.adminId}
      </div>
    );
  }
  return {
    ...actual,
    loadConsole: async () => ({ default: FakeConsole }),
  };
});

const { Boot } = await import("../Boot");

type Route = (init?: RequestInit) => Response | Promise<Response>;
let routes: Record<string, Route[]>;
const calls: string[] = [];

function serve(path: string, ...answers: Route[]) {
  routes[path] = answers;
}

beforeEach(() => {
  routes = {};
  calls.length = 0;
  kova.isSignedIn = false;
  kova.getToken.mockClear();
  kova.signOut.mockClear();
  consoleProps.latest = null;
  consoleProps.mounts = 0;
  window.history.replaceState(null, "", "/");
  sessionStorage.clear();
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const key = `${init?.method ?? "GET"} ${path}`;
      calls.push(key);
      const queue = routes[key];
      if (!queue?.length) return new Response(null, { status: 404 });
      const next = queue.length > 1 ? queue.shift()! : queue[0]!;
      return next(init);
    }),
  );
});

const bare404 = () => new Response(null, { status: 404 });
const adminSession = () =>
  Response.json({
    user_id: "admin-1",
    console: { script: "/assets/c.js", styles: [] },
  });
const granted = () => new Response(null, { status: 204 });

describe("the admin page", () => {
  it("opens the console straight away with an admin cookie", async () => {
    serve("GET /api/session", adminSession);
    render(<Boot />);
    expect((await screen.findByTestId("console")).textContent).toContain(
      "admin-1",
    );
    expect(screen.queryByLabelText("Kova sign-in")).toBeNull();
    expect(calls).toEqual(["GET /api/session"]);
  });

  it("shows only the sign-in form when signed out, never raw text", async () => {
    render(<Boot />);
    expect(await screen.findByLabelText("Kova sign-in")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Sign in" })).toBeTruthy();
  });

  it("goes from sign-in to the console in place, without a reload", async () => {
    serve("GET /api/session", bare404, adminSession);
    serve("POST /api/session", granted);
    const href = window.location.href;
    render(<Boot />);
    await screen.findByLabelText("Kova sign-in");
    act(() => kova.set({ isSignedIn: true }));
    expect(await screen.findByTestId("console")).toBeTruthy();
    expect(calls).toEqual([
      "GET /api/session",
      "POST /api/session",
      "GET /api/session",
    ]);
    expect(window.location.href).toBe(href);
    expect(consoleProps.latest).toMatchObject({ active: true, epoch: 1 });
  });

  it("refuses a non-admin after a second look, and signs Kova out in place", async () => {
    serve("GET /api/session", bare404);
    serve("POST /api/session", bare404);
    kova.isSignedIn = true;
    render(<Boot />);
    // The second look comes 1.5 s after the first refusal.
    expect(
      (await screen.findByRole("alert", {}, { timeout: 4000 })).textContent,
    ).toBe("That account can't sign in here.");
    expect(calls.filter((c) => c === "POST /api/session")).toHaveLength(2);
    // A fragment URL: Kova's redirect is a same-page navigation.
    expect(kova.signOut).toHaveBeenCalledWith(
      expect.stringMatching(/#signed-out$/),
    );
    expect(screen.queryByTestId("console")).toBeNull();
  });

  it("keeps the console and its route when the session ends mid-use", async () => {
    serve("GET /api/session", adminSession);
    window.history.replaceState(null, "", "/live");
    render(<Boot />);
    await screen.findByTestId("console");

    act(() => consoleProps.latest!.onSessionLost());
    expect(
      await screen.findByRole("heading", { name: "Your session ended" }),
    ).toBeTruthy();
    expect(screen.getByTestId("console").getAttribute("data-active")).toBe(
      "false",
    );
    expect(window.location.pathname).toBe("/live");

    // Signing in again in the prompt picks up where it was.
    serve("POST /api/session", granted);
    act(() => kova.set({ isSignedIn: true }));
    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { name: "Your session ended" }),
      ).toBeNull(),
    );
    expect(screen.getByTestId("console").getAttribute("data-active")).toBe(
      "true",
    );
    expect(consoleProps.latest!.epoch).toBe(2);
    expect(consoleProps.mounts).toBe(1);
    expect(window.location.pathname).toBe("/live");
  });

  it("re-verifies on its own when Kova is still signed in, and asks again if refused", async () => {
    serve("GET /api/session", adminSession);
    kova.isSignedIn = true;
    render(<Boot />);
    await screen.findByTestId("console");

    // The cookie lapsed but Kova's view is stale: the exchange is refused.
    serve("POST /api/session", bare404);
    kova.signOut.mockImplementation(async () => {
      kova.set({ isSignedIn: false });
    });
    act(() => consoleProps.latest!.onSessionLost());
    // Not a refusal page: the prompt, with the console still underneath.
    expect(
      await screen.findByLabelText("Kova sign-in", {}, { timeout: 4000 }),
    ).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: "Your session ended" }),
    ).toBeTruthy();
    expect(screen.getByLabelText("Kova sign-in")).toBeTruthy();
    expect(screen.getByTestId("console")).toBeTruthy();
    kova.signOut.mockImplementation(async () => {});
  });

  it("signs out in place", async () => {
    serve("GET /api/session", adminSession);
    serve("POST /api/sign-out", granted);
    kova.isSignedIn = true;
    window.history.replaceState(null, "", "/audit");
    render(<Boot />);
    await screen.findByTestId("console");
    kova.signOut.mockImplementation(async () => {
      kova.set({ isSignedIn: false });
    });
    act(() => consoleProps.latest!.onSignOut());
    expect(await screen.findByLabelText("Kova sign-in")).toBeTruthy();
    await waitFor(() => expect(window.location.pathname).toBe("/"));
    expect(calls).toContain("POST /api/sign-out");
    expect(screen.queryByTestId("console")).toBeNull();
    kova.signOut.mockImplementation(async () => {});
  });
});
