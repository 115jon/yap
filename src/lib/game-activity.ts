import type { GameActivity } from "@/lib/gateway-protocol";

/** "Playing Minecraft", as one plain string (row lines, labels). */
export function gameActivityLabel(activity: GameActivity): string {
  return `Playing ${activity.name}`;
}
