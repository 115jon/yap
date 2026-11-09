// @vitest-environment jsdom

import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(async (_options: unknown) => undefined),
  search: {} as Record<string, string | undefined>,
  getToken: vi.fn(async () => "tok_app"),
  setStoredKovaAuthSessionToken: vi.fn(),
  isSignedIn: true,
  signInProps: null as Record<string, unknown> | null,
}));

vi.mock("@tanstack/react-router", () => ({
  createFileRoute: () => (options: Record<string, unknown>) => ({
    options,
    useSearch: () => mocks.search,
  }),
  useNavigate: () => mocks.navigate,
}));

vi.mock("@kova/react", () => ({
  SignIn: (props: Record<string, unknown>) => {
    mocks.signInProps = props;
    return <div data-testid="sign-in-form" />;
  },
  useAuth: () => ({
    getToken: mocks.getToken,
    isLoaded: true,
    isSignedIn: mocks.isSignedIn,
  }),
}));

vi.mock("@/lib/desktop-auth", () => ({
  consumeAuthLogoutIntent: () => false,
  clearStoredKovaAuthSessionToken: vi.fn(),
  getStoredKovaAuthSessionToken: () => null,
  setStoredKovaAuthSessionToken: mocks.setStoredKovaAuthSessionToken,
}));

vi.mock("@/lib/platform", () => ({
  isTauri: () => false,
  getPublicWebUrl: () => "https://yap.115jon.com",
}));
vi.mock("@/components/DesktopLogin", () => ({ default: () => null }));
vi.mock("@/components/SplashScreen", () => ({
  SplashScreen: () => <div data-testid="splash" />,
}));
vi.mock("@/components/auth/AuthSplitLayout", () => ({
  AuthSplitLayout: ({
    children,
    subtitle,
  }: {
    children: React.ReactNode;
    subtitle?: React.ReactNode;
  }) => (
    <div>
      {subtitle}
      {children}
    </div>
  ),
}));
vi.mock("@/components/auth/auth-card-appearance", () => ({
  AUTH_CARD_APPEARANCE: {},
}));

import { Route } from "./sign-in";

const routeOptions = (
  Route as unknown as {
    options: {
      component: React.FC;
      validateSearch: (search: Record<string, unknown>) => {
        redirect_url?: string;
      };
    };
  }
).options;
const SignInPage = routeOptions.component;

beforeEach(() => {
  mocks.isSignedIn = true;
  mocks.navigate.mockClear();
  mocks.setStoredKovaAuthSessionToken.mockClear();
});

describe("/sign-in after signing in", () => {
  it("returns to /activate with its code (href, so the query survives)", async () => {
    mocks.search = { redirect_url: "/activate?code=ABCD-EFGH" };
    render(<SignInPage />);
    await waitFor(() =>
      expect(mocks.navigate).toHaveBeenCalledWith({
        href: "/activate?code=ABCD-EFGH",
        replace: true,
      }),
    );
    expect(mocks.setStoredKovaAuthSessionToken).toHaveBeenCalledWith("tok_app");
  });

  it("returns a guest to the room they signed in from", async () => {
    mocks.search = { redirect_url: "/room/standup" };
    render(<SignInPage />);
    await waitFor(() =>
      expect(mocks.navigate).toHaveBeenCalledWith({
        href: "/room/standup",
        replace: true,
      }),
    );
  });

  it("ignores a redirect_url that leaves the app", async () => {
    mocks.search = { redirect_url: "//evil.example/room/standup" };
    render(<SignInPage />);
    await waitFor(() =>
      expect(mocks.navigate).toHaveBeenCalledWith({
        href: "/chat",
        replace: true,
      }),
    );
  });

  it("goes to /chat without a redirect_url", async () => {
    mocks.search = {};
    render(<SignInPage />);
    await waitFor(() =>
      expect(mocks.navigate).toHaveBeenCalledWith({
        href: "/chat",
        replace: true,
      }),
    );
  });
});

describe("/sign-in reached from /activate", () => {
  it("says a device is waiting and shows its code", () => {
    mocks.isSignedIn = false;
    mocks.search = { redirect_url: "/activate?code=ABCD-EFGH" };
    render(<SignInPage />);
    expect(
      screen.getByText("A device is waiting to sign in"),
    ).toBeInTheDocument();
    expect(screen.getByText("ABCD-EFGH")).toBeInTheDocument();
  });

  it("stays generic for other return targets", () => {
    mocks.isSignedIn = false;
    mocks.search = { redirect_url: "/chat" };
    render(<SignInPage />);
    expect(screen.queryByText("A device is waiting to sign in")).toBeNull();
    expect(screen.getByTestId("sign-in-form")).toBeInTheDocument();
  });
});

describe("/sign-in reached from a guest room", () => {
  it("keeps redirect_url in the search params", () => {
    expect(
      routeOptions.validateSearch({ redirect_url: "/room/standup" }),
    ).toMatchObject({ redirect_url: "/room/standup" });
  });

  it("carries the room through the OAuth round trip", () => {
    mocks.isSignedIn = false;
    mocks.search = { redirect_url: "/room/standup" };
    render(<SignInPage />);
    // Kova sends the user back to this URL after Google; /sign-in then reads
    // redirect_url again and finishes the redirect above.
    expect(mocks.signInProps?.afterSignInUrl).toBe(
      "/sign-in?redirect_url=%2Froom%2Fstandup",
    );
    expect(mocks.signInProps?.signUpUrl).toBe(
      "/sign-up?redirect_url=%2Froom%2Fstandup",
    );
  });
});
