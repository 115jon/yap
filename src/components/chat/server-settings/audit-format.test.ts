import { describe, expect, it } from "vitest";

import { describeAudit, type AuditLookup } from "./audit-format";

const lookup: AuditLookup = {
  roles: new Map(),
  members: new Map([["user-2", "Sam"]]),
  channels: new Map([
    ["voice-a", { name: "General", voice: true }],
    ["voice-b", { name: "Lounge", voice: true }],
  ]),
};

describe("describeAudit MEMBER_MOVE", () => {
  it("reads as moved Sam from one voice channel to another", () => {
    expect(
      describeAudit(
        {
          id: "log-1",
          server_id: "server-1",
          actor_id: "mod-1",
          action_type: "MEMBER_MOVE",
          target_id: "user-2",
          changes: { from_channel_id: "voice-a", to_channel_id: "voice-b" },
          created_at: "2026-11-04T00:00:00Z",
        },
        lookup,
      ),
    ).toEqual({
      verb: "moved",
      target: { kind: "member", label: "Sam", named: true, id: "user-2" },
      details: [{ type: "text", text: "From General to Lounge" }],
    });
  });

  it("drops channel names it no longer knows", () => {
    expect(
      describeAudit(
        {
          id: "log-2",
          server_id: "server-1",
          actor_id: "mod-1",
          action_type: "MEMBER_MOVE",
          target_id: "user-2",
          changes: { from_channel_id: "gone", to_channel_id: "gone-too" },
          created_at: "2026-11-04T00:00:00Z",
        },
        lookup,
      ).details,
    ).toEqual([]);
  });
});

describe("describeAudit CHANNEL_UPDATE voice status", () => {
  const voiceStatusDetails = (voice_status: unknown) =>
    describeAudit(
      {
        id: "log-3",
        server_id: "server-1",
        actor_id: "mod-1",
        action_type: "CHANNEL_UPDATE",
        target_id: "voice-a",
        changes: { voice_status },
        created_at: "2026-11-04T00:00:00Z",
      },
      lookup,
    ).details;

  const clip = {
    id: "m1",
    provider: "klipy",
    media_type: "clips",
    preview_url: "/m1.mp4",
    preview_width: 320,
    preview_height: 180,
    preview_content_type: "video/mp4",
  };
  const gif = {
    ...clip,
    id: "m2",
    media_type: "gifs",
    preview_content_type: "image/gif",
  };

  it("reads a text status as its text, never [object Object]", () => {
    expect(
      voiceStatusDetails({
        old: null,
        new: { text: "Good morning", media: null },
      }),
    ).toEqual([{ type: "text", text: "Voice status: Good morning" }]);
  });

  it("reads a media-only status as a clip or a GIF", () => {
    expect(voiceStatusDetails({ old: null, new: { media: clip } })).toEqual([
      { type: "text", text: "Voice status: a clip" },
    ]);
    expect(voiceStatusDetails({ old: null, new: { media: gif } })).toEqual([
      { type: "text", text: "Voice status: a GIF" },
    ]);
  });

  it("prefers the media title and appends it to the text", () => {
    expect(
      voiceStatusDetails({
        old: null,
        new: { text: "hi", media: { ...gif, title: "Wave" } },
      }),
    ).toEqual([{ type: "text", text: "Voice status: hi (Wave)" }]);
    expect(
      voiceStatusDetails({ old: null, new: { text: "hi", media: gif } }),
    ).toEqual([{ type: "text", text: "Voice status: hi (a GIF)" }]);
  });

  it("shows the change from one status to another", () => {
    expect(
      voiceStatusDetails({
        old: { text: "Good morning", media: null },
        new: { text: null, media: clip },
      }),
    ).toEqual([{ type: "text", text: "Voice status: Good morning → a clip" }]);
  });

  it("still reads older entries that stored the bare text", () => {
    expect(voiceStatusDetails("Brb")).toEqual([
      { type: "text", text: "Voice status: Brb" },
    ]);
    expect(voiceStatusDetails({ old: "Brb", new: "Back" })).toEqual([
      { type: "text", text: "Voice status: Brb → Back" },
    ]);
  });

  it("reads an emptied status as cleared", () => {
    expect(voiceStatusDetails({ old: { text: "Brb" }, new: null })).toEqual([
      { type: "text", text: "Cleared the voice status" },
    ]);
    expect(
      voiceStatusDetails({
        old: { text: "Brb" },
        new: { text: " ", media: null },
      }),
    ).toEqual([{ type: "text", text: "Cleared the voice status" }]);
  });
});
