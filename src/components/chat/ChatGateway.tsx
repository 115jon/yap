import { getDesktopAuthHandoffToken } from "@/lib/desktop-auth";
import { onPageWake } from "@/lib/realtime-socket/page-wake";
import { useChatStore } from "@/stores/chat-store";
import { useAuth } from "@kova/react";
import { useEffect, useRef } from "react";

/**
 * Headless component that initializes and manages the
 * Chat WebSocket Gateway connection upon mount.
 *
 * Invite deep links (yap://invite/:code) are handled once, by
 * DesktopDeepLinkBridge in src/routes/__root.tsx.
 */
type ChatGatewayProps = {
  authenticatedUserId?: string | null;
};

export function ChatGateway({ authenticatedUserId }: ChatGatewayProps) {
  const { userId, isLoaded } = useAuth();
  const initGateway = useChatStore((s) => s.gateway.initGateway);
  const _setGatewayUserId = useChatStore((s) => s.gateway.setGatewayUserId);
  const disconnectGateway = useChatStore((s) => s.gateway.disconnectGateway);

  const tokenReady = isLoaded || !!getDesktopAuthHandoffToken();
  const routeUserId =
    authenticatedUserId === "web" || authenticatedUserId === "oauth-callback"
      ? null
      : authenticatedUserId;
  const gatewayUserId = userId ?? routeUserId;

  const disconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  useEffect(() => {
    // Wait for auth to settle before initialising the gateway: the DO needs
    // a real Kova user id for Identify.
    if (!tokenReady) return;
    if (!gatewayUserId) return;

    // If there was a pending disconnect from a strict-mode unmount, cancel it.
    if (disconnectTimeoutRef.current) {
      clearTimeout(disconnectTimeoutRef.current);
      disconnectTimeoutRef.current = null;
    }

    initGateway(gatewayUserId);

    return () => {
      // In React 18 strict mode, components unmount and remount immediately.
      // We delay the actual disconnect slightly so that if the component remounts
      // within that window, we preserve the active WebSocket session.
      disconnectTimeoutRef.current = setTimeout(() => {
        disconnectGateway();
      }, 500);
    };
  }, [initGateway, disconnectGateway, gatewayUserId, tokenReady]);

  useEffect(() => {
    useChatStore.getState().gateway.setGatewayUserId(gatewayUserId);
  }, [gatewayUserId]);

  // A backgrounded tab or a sleeping phone throttles or freezes timers, so a
  // pending retry can sit for its whole backoff (up to 30s) after the user is
  // back, and a socket that died in the background is only noticed after
  // three missed heartbeats. Wake the gateway the moment the page returns.
  // (Each SFUClient wakes its call sockets from the same page-wake source.)
  useEffect(() => onPageWake(() => useChatStore.getState().gateway.wake()), []);

  return null;
}
