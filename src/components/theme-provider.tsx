import { NEXT_THEME_NAMES } from "@/lib/theme-preferences";
import {
  isChatStoreLoaded,
  subscribeChatStoreLoaded,
} from "@/stores/chat-store-loaded";
import { ThemeProvider as NextThemesProvider } from "next-themes";
import * as React from "react";

// The bootstrap reads the signed-in user out of the chat store, which public
// pages never load (see chat-store-loaded.ts); it comes in with the store.
const ThemePreferenceBootstrap = React.lazy(
  () => import("@/components/ThemePreferenceBootstrap"),
);

const DEFAULT_THEME_PROVIDER_PROPS = {
  attribute: "class",
  // Signed-out visitors follow the OS scheme; a stored or synced choice wins.
  defaultTheme: "system",
  enableSystem: true,
  disableTransitionOnChange: true,
  themes: NEXT_THEME_NAMES,
} satisfies React.ComponentProps<typeof NextThemesProvider>;

export function ThemeProvider({
  children,
  ...props
}: React.ComponentProps<typeof NextThemesProvider>) {
  return (
    <NextThemesProvider {...DEFAULT_THEME_PROVIDER_PROPS} {...props}>
      <ThemePreferenceGate />
      {children}
    </NextThemesProvider>
  );
}

function ThemePreferenceGate() {
  const chatStoreLoaded = React.useSyncExternalStore(
    subscribeChatStoreLoaded,
    isChatStoreLoaded,
    () => false,
  );
  if (!chatStoreLoaded) return null;
  return (
    <React.Suspense fallback={null}>
      <ThemePreferenceBootstrap />
    </React.Suspense>
  );
}
