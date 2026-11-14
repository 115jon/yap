import { NotFoundPage, RouteErrorPage } from "@/components/PageState";
import { SplashScreen } from "@/components/SplashScreen";
import { ThemeProvider } from "@/components/theme-provider";
import { DeferredToaster } from "@/components/DeferredToaster";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useAppSessionRecovery } from "@/hooks/useAppSessionRecovery";
import {
  DEBUG_DESKTOP_AUTH,
  getDesktopAuthHandoffToken,
  handleKovaSessionTokenChange,
  subscribeDesktopTokenChanges,
  useKovaAuthTokenSync,
} from "@/lib/desktop-auth";
import { getKovaAuthConfig } from "@/lib/kova-auth-config";
import { installMediaCapRepair } from "@/lib/media-repair";
import { getPublicWebUrl } from "@/lib/platform";
import { KovaAuthProvider, useAuth } from "@kova/react";
import {
  HeadContent,
  Outlet,
  Scripts,
  createRootRoute,
} from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { clog } from "@/lib/console-logger";
import appCss from "../styles.css?url";
import { AppActiveRoot } from "@/hooks/useAppActive";
import geistFont from "@fontsource-variable/geist/files/geist-latin-wght-normal.woff2?url";
import bricolageFont from "@fontsource-variable/bricolage-grotesque/files/bricolage-grotesque-latin-opsz-normal.woff2?url";
import martianFont from "@fontsource-variable/martian-mono/files/martian-mono-latin-wdth-normal.woff2?url";

const authLog = clog("DesktopAuth");
const faviconHref = import.meta.env.DEV ? "/favicon-dev.ico" : "/favicon.ico";
const SITE_TITLE = "Yap: voice, chat and video rooms";
const SITE_DESCRIPTION =
  "Voice, chat, video and screen share for the people you talk to every day. Start a room, send the link, they're in.";

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      {
        name: "viewport",
        // viewport-fit=cover makes env(safe-area-inset-*) non-zero (notch,
        // home indicator); resizes-content shrinks the layout for the
        // on-screen keyboard so the composer stays above it.
        content:
          "width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content",
      },
      { title: SITE_TITLE },
      { name: "description", content: SITE_DESCRIPTION },
      // Link previews: Yappy and the wordmark on the dark panel (1200x630).
      // No og:image size or alt here: a route's own og:image (a shared
      // message) replaces this one, and would inherit them.
      { property: "og:type", content: "website" },
      { property: "og:site_name", content: "Yap" },
      { property: "og:title", content: SITE_TITLE },
      { property: "og:description", content: SITE_DESCRIPTION },
      { property: "og:image", content: `${getPublicWebUrl()}/og-image.png` },
      { name: "twitter:card", content: "summary_large_image" },
    ],
    links: [
      { rel: "icon", href: faviconHref },
      { rel: "apple-touch-icon", href: "/apple-touch-icon.png" },
      { rel: "manifest", href: "/manifest.webmanifest" },
      // Self-hosted faces (styles.css imports the @font-face rules); the
      // latin subsets of the faces on first paint are preloaded so text doesn't
      // swap (Geist Mono is code only and loads on demand).
      {
        rel: "preload",
        href: geistFont,
        as: "font",
        type: "font/woff2",
        crossOrigin: "anonymous",
      },
      // Display (titles, names, control labels) and silkscreen (labels,
      // readouts, counts) are on first paint of every surface too.
      {
        rel: "preload",
        href: bricolageFont,
        as: "font",
        type: "font/woff2",
        crossOrigin: "anonymous",
      },
      {
        rel: "preload",
        href: martianFont,
        as: "font",
        type: "font/woff2",
        crossOrigin: "anonymous",
      },
      { rel: "stylesheet", href: appCss },
    ],
  }),
  component: RootComponent,
  pendingComponent: SplashScreen,
  shellComponent: RootDocument,
  errorComponent: RouteErrorPage,
  notFoundComponent: NotFoundPage,
});

function RootDocument({ children }: { children: React.ReactNode }) {
  // TanStack Start SSR root
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body
        className="bg-ply-0 text-ink-1 antialiased font-ui"
        data-gramm="false"
        data-gramm_editor="false"
        data-enable-grammarly="false"
        suppressHydrationWarning
      >
        {children}
        <Scripts />
      </body>
    </html>
  );
}

function RootComponent() {
  // Private media that failed unsigned or expired is signed again.
  useEffect(() => installMediaCapRepair(), []);
  const [sessionToken, setSessionToken] = useState<string | undefined>();
  const sessionTokenStateRef = useRef({ seenSessionToken: false });

  useEffect(() => {
    // Auth handoff storage is intentionally read after the stable server snapshot.
    // eslint-disable-next-line react-hooks-js/set-state-in-effect
    setSessionToken(getDesktopAuthHandoffToken() ?? undefined);
    return subscribeDesktopTokenChanges((token) => {
      setSessionToken(token ?? undefined);
    });
  }, []);

  const content = (
    <ThemeProvider>
      <AppActiveRoot />
      <TooltipProvider>
        <Outlet />
        <DeferredToaster />
      </TooltipProvider>
    </ThemeProvider>
  );

  return (
    <KovaAuthProvider
      {...getKovaAuthConfig()}
      afterSignInUrl="/chat"
      afterSignOutUrl="/sign-in"
      initialSessionToken={sessionToken}
      onSessionTokenChange={(token: string | null) => {
        setSessionToken(token ?? undefined);
        if (DEBUG_DESKTOP_AUTH) {
          authLog.info("KovaAuthProvider session token changed", {
            hasToken: !!token,
            tokenLength: token?.length ?? 0,
          });
        }
        handleKovaSessionTokenChange(token, sessionTokenStateRef.current);
      }}
    >
      <KovaMeetTokenBridge />
      {content}
    </KovaAuthProvider>
  );
}

function KovaMeetTokenBridge() {
  const { getToken, isLoaded, isSignedIn } = useAuth();
  useKovaAuthTokenSync(getToken, isLoaded, isSignedIn);
  // A stored token Kova no longer knows: mint a new one from the cookie.
  useAppSessionRecovery();
  return null;
}
