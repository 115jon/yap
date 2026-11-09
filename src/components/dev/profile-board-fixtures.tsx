/**
 * Dev-only fixtures for profile boards (mounted by UiFixtures for every
 * `pb-*` surface). Each family lives in its own module under
 * `dev/profile-board/` and is loaded on demand:
 *
 *   pb-board*            the Rack viewer (board-fixtures)
 *   pb-activity*, pb-wishlist*  the Activity and Wishlist tabs (tab-fixtures)
 *   pb-edit*, pb-studio*  edit in place and the Studio (editor-fixtures)
 *
 * The owning module documents its surfaces and query params.
 */

import { lazy, Suspense } from "react";

const BoardFixtures = lazy(() => import("./profile-board/board-fixtures"));
const TabFixtures = lazy(() => import("./profile-board/tab-fixtures"));
const EditorFixtures = lazy(() => import("./profile-board/editor-fixtures"));

export function isProfileBoardFixture(surface: string): boolean {
  return surface.startsWith("pb-");
}

export function ProfileBoardFixture({ surface }: { surface: string }) {
  const Fixture =
    surface.startsWith("pb-activity") || surface.startsWith("pb-wishlist")
      ? TabFixtures
      : surface.startsWith("pb-edit") || surface.startsWith("pb-studio")
        ? EditorFixtures
        : BoardFixtures;
  return (
    <Suspense fallback={null}>
      <Fixture surface={surface} />
    </Suspense>
  );
}
