// Compile-time checks that the web and the native client agree on the wire.
// `pnpm typecheck` runs them; there is nothing to execute.
//
// packages/protocol/src/generated holds TypeScript written from the Rust
// types in native/rust/crates/yap-protocol (`pnpm protocol:generate`, checked
// by CI's rust-shared job). Those types say what the native client accepts:
// a field it can do without is optional there. So the checks run one way per
// direction:
//
// - server -> native: what the server sends (the web's own types, which the
//   routes build their responses with) must be assignable to what native
//   reads. A renamed or dropped field that native needs fails here.
// - native -> server: what native sends (the Rust payloads, `x?: T` where
//   they skip `None`) must be assignable to what the gateway's schemas take.
//
// A failure means the two clients disagree: fix the side that drifted, or the
// Rust type when native was right to change.

import type {
  ChatGatewayServerMessage,
  GameActivity as WebGameActivity,
  GatewayOp,
  MainClientPayload,
  VoiceClientPayload,
} from "@ralph/protocol";
import type * as Wire from "@ralph/protocol/generated/game-activity";
import type * as Board from "@ralph/protocol/generated/profile-board";
import type * as Payload from "@ralph/protocol/generated/payload";
import type * as Tiers from "@ralph/protocol/generated/tiers";
import type { UserBadge as WebUserBadge } from "@/lib/badges";
import type {
  EntitlementSource as WebEntitlementSource,
  Tier as WebTier,
} from "@/lib/entitlements";
import type * as WebBoard from "@/lib/profile-board";
import type { GuestbookPage as WebGuestbookPage } from "@/lib/profile-board-api";

/**
 * `true` when every `A` is a `B`. `B` is compared read-only: the web's
 * schema types have readonly arrays, and a reader never writes to its input.
 */
type Assignable<A, B> = [A] extends [DeepReadonly<B>] ? true : false;
type DeepReadonly<T> = T extends (infer E)[]
  ? readonly DeepReadonly<E>[]
  : T extends object
    ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
    : T;
type Check<T extends true> = T;
/** `true` when `A` and `B` are the same type. */
type Same<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2
    ? true
    : false;

// ── Shared outright ───────────────────────────────────────────────────────
// The web's `Entitlement` is the generated one (src/lib/entitlements.ts);
// the lists it is built from must name the same values.

export type SharedOutright = [
  Check<Same<WebTier, Tiers.Tier>>,
  Check<Same<WebEntitlementSource, Tiers.EntitlementSource>>,
];

// ── server -> native: REST bodies ─────────────────────────────────────────

export type ServerToNative = [
  // Badges (users/me, profiles).
  Check<Assignable<WebUserBadge, Tiers.UserBadge>>,

  // Profile boards (/api/users/:id/board and friends).
  Check<Assignable<WebBoard.ProfileBoard, Board.Board>>,
  Check<Assignable<WebBoard.BoardResponse, Board.BoardResponse>>,
  Check<Assignable<WebBoard.BoardSaveResponse, Board.BoardSaveResponse>>,
  Check<Assignable<WebBoard.BoardData, Board.BoardData>>,
  Check<Assignable<WebBoard.BoardServer, Board.BoardServer>>,
  Check<Assignable<WebBoard.BoardUserRef, Board.BoardUserRef>>,
  Check<Assignable<WebBoard.GuestbookNote, Board.GuestbookNote>>,
  Check<Assignable<WebGuestbookPage, Board.GuestbookPage>>,
  Check<Assignable<WebBoard.DroppedWidget, Board.DroppedWidget>>,
  Check<Assignable<WebBoard.ActivityEvent, Board.ActivityEvent>>,
  Check<Assignable<WebBoard.ActivitySettings, Board.ActivitySettings>>,
  Check<Assignable<WebBoard.ActivityPage, Board.ActivityPage>>,
  Check<Assignable<WebBoard.ProfilePrefs, Board.ProfilePrefs>>,
  Check<Assignable<WebBoard.WishlistItem, Board.WishlistItem>>,
];

// ── server -> native: the chat socket ─────────────────────────────────────

type ServerD<Op extends GatewayOp> = Extract<
  ChatGatewayServerMessage,
  { op: Op }
>["d"];

export type ChatSocketToNative = [
  Check<Assignable<ServerD<GatewayOp.Hello>, Payload.Hello>>,
  Check<Assignable<ServerD<GatewayOp.Error>, Payload.GatewayError>>,
  Check<Assignable<ServerD<GatewayOp.Ready>, Payload.ChatReady>>,
];

// ── native -> server: gateway client payloads ─────────────────────────────

export type NativeToMainGateway = [
  Check<Assignable<Payload.Identify, MainClientPayload<GatewayOp.Identify>>>,
  Check<Assignable<Payload.Resume, MainClientPayload<GatewayOp.Resume>>>,
  Check<
    Assignable<
      Payload.ChannelRef,
      MainClientPayload<GatewayOp.ChannelSubscribe>
    >
  >,
  Check<
    Assignable<
      Payload.ChannelRef,
      MainClientPayload<GatewayOp.ChannelUnsubscribe>
    >
  >,
  Check<
    Assignable<
      Payload.PresenceStatus,
      MainClientPayload<GatewayOp.PresenceUpdate>
    >
  >,
  Check<
    Assignable<
      Payload.ServerSubscribe,
      MainClientPayload<GatewayOp.ServerSubscribe>
    >
  >,
  Check<
    Assignable<
      Payload.VoiceChannelJoin,
      MainClientPayload<GatewayOp.VoiceChannelJoin>
    >
  >,
  Check<
    Assignable<
      Payload.VoiceChannelLeave,
      MainClientPayload<GatewayOp.VoiceChannelLeave>
    >
  >,
  Check<
    Assignable<
      Payload.ClientVoiceState,
      MainClientPayload<GatewayOp.VoiceStateUpdate>
    >
  >,
  Check<
    Assignable<Payload.CallInitiate, MainClientPayload<GatewayOp.CallInitiate>>
  >,
  Check<Assignable<Payload.CallRef, MainClientPayload<GatewayOp.CallAccept>>>,
  Check<Assignable<Payload.CallRef, MainClientPayload<GatewayOp.CallDecline>>>,
  Check<Assignable<Payload.CallRef, MainClientPayload<GatewayOp.CallEnd>>>,
  // The game activity native detects ("Playing X").
  Check<Assignable<Wire.GameActivity, WebGameActivity>>,
  Check<
    Assignable<Wire.ActivityUpdate, MainClientPayload<GatewayOp.ActivityUpdate>>
  >,
];

// Video (op 12) is left out: yap-voice writes its frames by hand
// (wire.rs), not from `payload::Video`.
export type NativeToVoiceGateway = [
  Check<
    Assignable<
      Payload.VoiceIdentify,
      VoiceClientPayload<GatewayOp.VoiceIdentify>
    >
  >,
  Check<
    Assignable<Payload.StopTracks, VoiceClientPayload<GatewayOp.StopTracks>>
  >,
  Check<Assignable<Payload.Speaking, VoiceClientPayload<GatewayOp.Speaking>>>,
  Check<
    Assignable<
      Payload.TrackUnavailable,
      VoiceClientPayload<GatewayOp.TrackUnavailable>
    >
  >,
];
