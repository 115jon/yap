// Compile-time checks that the web and the native client agree on the wire.
// `pnpm typecheck` runs them; there is nothing to execute.
//
// packages/protocol/src/generated holds TypeScript written from the Rust
// types in native/rust/crates/yap-protocol (`pnpm protocol:generate`, checked
// by CI's rust-shared job). Those types say what the native client accepts:
// a field it can do without is optional there. So the checks run one way per
// direction:
//
// - server -> native (`Reads`): what the server sends (the web's own types,
//   which the routes build their responses with) must be assignable to what
//   native reads, and native must read no field the server's type lacks. A
//   renamed or dropped field, optional or not, fails here.
// - native -> server (`Sends`): what native sends (the Rust payloads, `x?: T`
//   where they skip `None`) must be assignable to what the gateway's schemas
//   take, and name no field the schema lacks (the gateway would drop it).
//
// Fields one side has on purpose and the other lacks are listed per check.
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
// Every generated module is imported, so a broken one fails typecheck.
import type * as Collectibles from "@ralph/protocol/generated/collectibles";
import type * as Frame from "@ralph/protocol/generated/frame";
import type * as Wire from "@ralph/protocol/generated/game-activity";
import type * as Payload from "@ralph/protocol/generated/payload";
import type * as Board from "@ralph/protocol/generated/profile-board";
import type * as Rest from "@ralph/protocol/generated/rest";
import type * as Tiers from "@ralph/protocol/generated/tiers";
import type * as VoiceApp from "@ralph/protocol/generated/voice-app";
import type { UserBadge as WebUserBadge } from "@/lib/badges";
import type {
  EntitlementSource as WebEntitlementSource,
  Tier as WebTier,
} from "@/lib/entitlements";
import type * as WebBoard from "@/lib/profile-board";
import type { GuestbookPage as WebGuestbookPage } from "@/services/profile-guestbook.service";

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
/** `true` when `A` has no key outside `B` and `Extra`. */
type NoExtraKeys<A, B, Extra extends PropertyKey> = [
  Exclude<keyof A, keyof B | Extra>,
] extends [never]
  ? true
  : false;
/** The server's `Server` is what native reads as `Native`. */
type Reads<Server, Native, NotSent extends PropertyKey = never> =
  Assignable<Server, Native> extends true
    ? NoExtraKeys<Native, Server, NotSent>
    : false;
/** What native sends as `Native` is what the gateway takes as `Schema`. */
type Sends<Native, Schema, NotTaken extends PropertyKey = never> =
  Assignable<Native, Schema> extends true
    ? NoExtraKeys<Native, Schema, NotTaken>
    : false;
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
  Check<Reads<WebUserBadge, Tiers.UserBadge>>,

  // Profile boards (/api/users/:id/board and friends).
  Check<Reads<WebBoard.ProfileBoard, Board.Board>>,
  Check<Reads<WebBoard.BoardResponse, Board.BoardResponse>>,
  Check<Reads<WebBoard.BoardSaveResponse, Board.BoardSaveResponse>>,
  Check<Reads<WebBoard.BoardData, Board.BoardData>>,
  Check<Reads<WebBoard.BoardServer, Board.BoardServer>>,
  Check<Reads<WebBoard.BoardUserRef, Board.BoardUserRef>>,
  Check<Reads<WebBoard.GuestbookNote, Board.GuestbookNote>>,
  Check<Reads<WebGuestbookPage, Board.GuestbookPage>>,
  Check<Reads<WebBoard.DroppedWidget, Board.DroppedWidget>>,
  Check<Reads<WebBoard.ActivityEvent, Board.ActivityEvent>>,
  Check<Reads<WebBoard.ActivitySettings, Board.ActivitySettings>>,
  Check<Reads<WebBoard.ActivityPage, Board.ActivityPage>>,
  Check<Reads<WebBoard.ProfilePrefs, Board.ProfilePrefs>>,
  Check<Reads<WebBoard.WishlistItem, Board.WishlistItem>>,
];

// ── The chat socket: what the web client accepts, native accepts too ──────
// `chatGatewayServerRules` are the web client's read schemas, deliberately
// looser than what the Durable Objects send (server-payloads.ts). Native
// must not need more than they guarantee.

type ServerD<Op extends GatewayOp> = Extract<
  ChatGatewayServerMessage,
  { op: Op }
>["d"];

export type ChatSocketReaders = [
  Check<Reads<ServerD<GatewayOp.Hello>, Payload.Hello>>,
  Check<Reads<ServerD<GatewayOp.Error>, Payload.GatewayError>>,
  Check<Reads<ServerD<GatewayOp.Ready>, Payload.ChatReady>>,
];

// ── native -> server: gateway client payloads ─────────────────────────────

export type NativeToMainGateway = [
  // `clerk_user_id`: the global gateway's Identify still carries it, and the
  // gateway drops it.
  Check<
    Sends<
      Payload.Identify,
      MainClientPayload<GatewayOp.Identify>,
      "clerk_user_id"
    >
  >,
  Check<Sends<Payload.Resume, MainClientPayload<GatewayOp.Resume>>>,
  Check<
    Sends<Payload.ChannelRef, MainClientPayload<GatewayOp.ChannelSubscribe>>
  >,
  Check<
    Sends<Payload.ChannelRef, MainClientPayload<GatewayOp.ChannelUnsubscribe>>
  >,
  Check<
    Sends<Payload.PresenceStatus, MainClientPayload<GatewayOp.PresenceUpdate>>
  >,
  Check<
    Sends<Payload.ServerSubscribe, MainClientPayload<GatewayOp.ServerSubscribe>>
  >,
  Check<
    Sends<
      Payload.VoiceChannelJoin,
      MainClientPayload<GatewayOp.VoiceChannelJoin>
    >
  >,
  Check<
    Sends<
      Payload.VoiceChannelLeave,
      MainClientPayload<GatewayOp.VoiceChannelLeave>
    >
  >,
  Check<
    Sends<
      Payload.ClientVoiceState,
      MainClientPayload<GatewayOp.VoiceStateUpdate>
    >
  >,
  Check<Sends<Payload.CallInitiate, MainClientPayload<GatewayOp.CallInitiate>>>,
  Check<Sends<Payload.CallRef, MainClientPayload<GatewayOp.CallAccept>>>,
  Check<Sends<Payload.CallRef, MainClientPayload<GatewayOp.CallDecline>>>,
  Check<Sends<Payload.CallRef, MainClientPayload<GatewayOp.CallEnd>>>,
  // The game activity native detects ("Playing X").
  Check<Sends<Wire.GameActivity, WebGameActivity>>,
  Check<
    Sends<Wire.ActivityUpdate, MainClientPayload<GatewayOp.ActivityUpdate>>
  >,
];

// Video (op 12) is left out: yap-voice writes its frames by hand
// (wire.rs), not from `payload::Video`.
export type NativeToVoiceGateway = [
  Check<
    Sends<Payload.VoiceIdentify, VoiceClientPayload<GatewayOp.VoiceIdentify>>
  >,
  // The relay's own fields: native leaves them unset, so they are skipped.
  Check<
    Sends<
      Payload.StopTracks,
      VoiceClientPayload<GatewayOp.StopTracks>,
      "participant_id" | "tracks"
    >
  >,
  Check<
    Sends<
      Payload.Speaking,
      VoiceClientPayload<GatewayOp.Speaking>,
      "participant_id"
    >
  >,
  Check<
    Sends<
      Payload.TrackUnavailable,
      VoiceClientPayload<GatewayOp.TrackUnavailable>
    >
  >,
];

// Modules no check above uses yet, kept in the program.
export type Unchecked = [
  Collectibles.CatalogResponse,
  Frame.Frame,
  Rest.Message,
  VoiceApp.LtSnapshot,
];
