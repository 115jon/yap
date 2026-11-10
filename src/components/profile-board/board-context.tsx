/**
 * Where board media resolves: uploaded widget images and server media
 * paths go through the authenticated asset URL by default; fixtures swap
 * in bundled pictures with BoardMediaProvider.
 */

import { createContext, useContext, useMemo, type ReactNode } from "react";

import { getAuthAssetUrl } from "@/lib/platform";
import { widgetAssetUrl } from "@/lib/profile-board";

export interface BoardMedia {
  /** An uploaded widget image of `ownerId`. */
  asset(ownerId: string, asset: string): string;
  /** A server-given media path (game icons, server icons, avatars). */
  url(path: string): string;
}

const DEFAULT_MEDIA: BoardMedia = {
  asset: (ownerId, asset) => getAuthAssetUrl(widgetAssetUrl(ownerId, asset)),
  url: (path) => getAuthAssetUrl(path),
};

const BoardMediaContext = createContext<BoardMedia>(DEFAULT_MEDIA);

/**
 * Overrides where board media resolves. Fixtures only: they point asset
 * names and icon paths at bundled images instead of the API.
 */
export function BoardMediaProvider({
  asset,
  url,
  children,
}: Partial<BoardMedia> & { children: ReactNode }) {
  const value = useMemo<BoardMedia>(
    () => ({
      asset: asset ?? DEFAULT_MEDIA.asset,
      url: url ?? DEFAULT_MEDIA.url,
    }),
    [asset, url],
  );
  return (
    <BoardMediaContext.Provider value={value}>
      {children}
    </BoardMediaContext.Provider>
  );
}

export function useBoardMedia(): BoardMedia {
  return useContext(BoardMediaContext);
}
