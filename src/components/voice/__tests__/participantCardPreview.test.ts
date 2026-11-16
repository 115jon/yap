// Frontend unit tests for the preview-paused placeholder.
//
// Validates: Requirement 5.2 (the paused placeholder renders).
//
// The repo's vitest setup runs in the "node" environment with no DOM testing
// stack, so the REAL `ParticipantCard` is server-rendered with
// `react-dom/server` and the "Preview paused" placeholder is asserted present
// when `isPreviewHidden` is true (and absent when false).

import { describe, expect, it } from "vitest";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { ParticipantCard } from "@/components/voice/ParticipantCard";
import type { GridItem, VoiceActions } from "@/components/voice/types";

function makeLocalScreenItem(overrides: Partial<GridItem> = {}): GridItem {
  return {
    id: "local-screen-1",
    userId: "user-1",
    name: "You",
    avatar: null,
    stream: null,
    isLocal: true,
    type: "screen",
    isStreaming: true,
    isMuted: false,
    isDeafened: false,
    isSpeaking: false,
    ...overrides,
  };
}

function renderCard(item: GridItem, voiceActions: VoiceActions): string {
  return renderToStaticMarkup(
    React.createElement(ParticipantCard, {
      item,
      isFocused: false,
      isTray: false,
      globalDeafened: false,
      onClick: () => {},
      voiceActions,
      watchedStreams: {},
      streamThumbnails: {},
    }),
  );
}

describe("ParticipantCard — preview-paused placeholder (Req 5.2)", () => {
  it("renders the 'Preview paused' placeholder when the local screen tile is hidden", () => {
    const markup = renderCard(makeLocalScreenItem(), {
      isPreviewHidden: true,
      togglePreviewHidden: () => {},
    });

    expect(markup).toContain("Preview paused");
  });

  it("does not render the placeholder when the preview is shown", () => {
    const markup = renderCard(makeLocalScreenItem({ stream: null }), {
      isPreviewHidden: false,
      togglePreviewHidden: () => {},
    });

    expect(markup).not.toContain("Preview paused");
  });

  it("does not render the placeholder for a remote screen tile even if hidden flag is set", () => {
    const markup = renderCard(makeLocalScreenItem({ isLocal: false }), {
      isPreviewHidden: true,
      togglePreviewHidden: () => {},
    });

    // The placeholder is local-only (isScreen && item.isLocal && isPreviewHidden).
    expect(markup).not.toContain("Preview paused");
  });

  it("keeps the preview toggle above the tile activation layer", () => {
    const markup = renderCard(makeLocalScreenItem(), {
      isPreviewHidden: false,
      togglePreviewHidden: () => {},
    });

    expect(markup).toMatch(
      /aria-label="Open participant tile for You"[^>]*class="[^"]*z-0/,
    );
    expect(markup).toMatch(/data-preview-toggle="true"[^>]*class="[^"]*z-10/);
  });
});

describe("ParticipantCard — a share's stream before its video arrives", () => {
  const liveAudio = { kind: "audio", readyState: "live" } as MediaStreamTrack;
  const audioOnly = {
    getAudioTracks: () => [liveAudio],
    getVideoTracks: () => [],
    getTracks: () => [liveAudio],
  } as unknown as MediaStream;
  const remoteShare = makeLocalScreenItem({
    id: "remote-screen-user-2",
    userId: "user-2",
    name: "Alice",
    isLocal: false,
    stream: audioOnly,
  });
  const render = (watched: boolean) =>
    renderToStaticMarkup(
      React.createElement(ParticipantCard, {
        item: remoteShare,
        isFocused: false,
        isTray: false,
        globalDeafened: false,
        onClick: () => {},
        voiceActions: {},
        watchedStreams: watched ? { "user-2": true } : {},
        streamThumbnails: {},
      }),
    );

  it("renders no video element for an unwatched share that only has audio", () => {
    const markup = render(false);
    expect(markup).not.toContain("<video");
    expect(markup).toContain("Watch stream");
  });

  it("renders no video under the Watch prompt even when the share's video is pulled", () => {
    const liveVideo = { kind: "video", readyState: "live" } as MediaStreamTrack;
    const markup = renderToStaticMarkup(
      React.createElement(ParticipantCard, {
        item: {
          ...remoteShare,
          stream: {
            getAudioTracks: () => [liveAudio],
            getVideoTracks: () => [liveVideo],
            getTracks: () => [liveAudio, liveVideo],
          } as unknown as MediaStream,
        },
        isFocused: false,
        isTray: false,
        globalDeafened: false,
        onClick: () => {},
        voiceActions: {},
        watchedStreams: {},
        streamThumbnails: {},
      }),
    );
    expect(markup).not.toContain("<video");
    expect(markup).toContain("Watch stream");
  });

  it("shows the loader, not a black video, while a watched share's video is pulled", () => {
    const markup = render(true);
    expect(markup).not.toContain("<video");
    expect(markup).toContain('role="status"');
  });
});
