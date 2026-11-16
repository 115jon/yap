// The voice header's connection word (an LED label).

import { describe, expect, it } from "vitest";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { VoiceConnectionLabel } from "@/components/voice/VoiceConnectionLabel";

function render(props: { connectionState: string; joined: boolean }) {
  return renderToStaticMarkup(React.createElement(VoiceConnectionLabel, props));
}

describe("VoiceConnectionLabel", () => {
  it("names the connection state", () => {
    expect(render({ connectionState: "connected", joined: true })).toContain(
      "Connected",
    );
    expect(render({ connectionState: "connecting", joined: false })).toContain(
      "Connecting",
    );
    expect(render({ connectionState: "failed", joined: false })).toContain(
      "Failed",
    );
  });

  it("lights reconnecting amber and failed red, as an LED label word", () => {
    const reconnecting = render({
      connectionState: "reconnecting",
      joined: true,
    });
    expect(reconnecting).toContain("Reconnecting");
    expect(reconnecting).toContain("text-warning-ink");
    expect(reconnecting).toContain('data-state="idle"');

    const failed = render({ connectionState: "failed", joined: true });
    expect(failed).toContain("text-danger-ink");
    expect(failed).toContain('data-state="muted"');
    expect(failed).not.toContain("Stable");
  });
});
