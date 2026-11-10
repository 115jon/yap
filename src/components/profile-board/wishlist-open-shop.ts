/**
 * "Open in the shop" from a wishlist card: closes the profile and shows the
 * shop on the DM home, the same steps as the Shop row in the DM sidebar.
 * The shop has no deep link to one item yet, so it opens on its front page.
 *
 * The chat store is imported on demand so the Wishlist chunk stays small
 * (the store is already loaded wherever a profile can open).
 */
export async function openShopFromWishlist(): Promise<void> {
  const { useChatStore } = await import("@/stores/chat-store");
  const { dispatch } = useChatStore.getState();
  dispatch({ type: "SET_PROFILE_USER", user: null });
  dispatch({ type: "SET_DM_HOME_VIEW", view: "shop" });
  dispatch({ type: "SET_ACTIVE_SERVER", serverId: "@me" });
  dispatch({ type: "SET_ACTIVE_CHANNEL", channelId: null });
}
