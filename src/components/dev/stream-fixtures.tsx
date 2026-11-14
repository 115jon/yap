/**
 * Dev-only screen share fixtures (mounted by UiFixtures), each open over the
 * chat shell so they can be shot at 360 / 390 / 768 / 1280.
 *
 *   st-share-web              browser share (quality + audio, then getDisplayMedia)
 *   st-share-unsupported      phone browser: explains sharing isn't available
 */

import { ScreenShareModal } from "@/components/ScreenShareModal";
import { UnifiedScreenShareModal } from "@/components/UnifiedScreenShareModal";
import type { ReactNode } from "react";

const QUALITIES = [
  "720p30",
  "720p60",
  "1080p30",
  "1080p60",
  "1440p30",
  "1440p60",
  "4k30",
  "4k60",
];
const noop = () => {};

export function StreamFixture({
  surface,
  shell,
}: {
  surface: string;
  shell: ReactNode;
}) {
  let overlay: ReactNode = null;
  switch (surface) {
    case "st-share-web":
      overlay = (
        <ScreenShareModal
          isOpen
          onClose={noop}
          onStart={noop}
          availableQualities={QUALITIES}
        />
      );
      break;
    case "st-share-unsupported":
      overlay = (
        <UnifiedScreenShareModal
          isOpen
          onClose={noop}
          onStart={noop}
          availableQualities={QUALITIES}
          support={{
            supported: false,
            reason: "mobile-browser",
          }}
        />
      );
      break;
    default:
      overlay = (
        <p className="p-6 text-ui-sm text-rm-text">Unknown surface {surface}</p>
      );
  }
  return (
    <div className="relative h-full w-full">
      {shell}
      {overlay}
    </div>
  );
}
