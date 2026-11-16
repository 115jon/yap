import { cn } from "@/lib/utils";
import { LedLabel, type LedState } from "@/components/ui/led";
import {
  connectionStatus,
  type ConnectionTone,
} from "@/lib/voice/connection-status-label";

/**
 * The connection word is a status LED label (DESIGN.md, LED label): connected
 * lights talk green, reconnecting amber, a failure muted red, and a state
 * still on its way stays unlit. Over focused media (`emphasized`) the word
 * stays white so it reads on the black gradient; the LED still carries the
 * state.
 */
const TONE_LED: Record<ConnectionTone, LedState> = {
  good: "talk",
  pending: "off",
  warning: "idle",
  danger: "muted",
};

export function VoiceConnectionLabel({
  connectionState,
  joined,
  emphasized = false,
  className,
}: {
  /** WebRTC connection state string (e.g. `"connected"`, `"new"`, `"failed"`). */
  connectionState: string;
  /** Whether the local user has joined the voice/room session. */
  joined: boolean;
  /** Over focused media the word stays white (the header's `focusedItem`). */
  emphasized?: boolean;
  className?: string;
}) {
  const { label, tone } = connectionStatus(connectionState, joined);
  return (
    <LedLabel
      role="status"
      state={TONE_LED[tone]}
      className={cn(
        "whitespace-nowrap",
        emphasized && "text-white/80",
        className,
      )}
    >
      {/* A phone header has no room for "Connected": the lit LED says it and
          the word stays for screen readers. Anything else keeps its word. */}
      <span className={cn(tone === "good" && "max-sm:sr-only")}>{label}</span>
    </LedLabel>
  );
}
