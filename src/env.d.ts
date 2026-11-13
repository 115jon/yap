/// <reference types="vite/client" />

import type { BackgroundTaskEnvelope } from "./lib/background-tasks";

// Augment the CloudflareEnv interface with our custom bindings
// These are defined in wrangler.toml and injected at runtime

declare global {
  /**
   * Build flags, defined by each Vite config (web, desktop, mobile). Read
   * them through isDesktop()/isMobile() in src/lib/runtime.ts.
   */
  const __IS_DESKTOP__: boolean;
  const __IS_MOBILE__: boolean;

  interface ImportMetaEnv {
    /** `vite dev` only: the Kova key RALPH_AUTH picked (lib/dev-auth-target). */
    readonly RALPH_DEV_KOVA_KEY?: string;
    /** The commit and time of the build (vite.config.ts), for /admin. */
    readonly YAP_BUILD_COMMIT?: string;
    readonly YAP_BUILT_AT?: string;
  }

  interface CloudflareEnv {
    DB: D1Database;
    BUCKET: R2Bucket;
    CACHE: KVNamespace;
    MEETING_ROOM: DurableObjectNamespace;
    VOICE_ROOM: DurableObjectNamespace;
    RATE_LIMITER: DurableObjectNamespace;
    BACKGROUND_TASKS?: Queue<BackgroundTaskEnvelope>;
    INSTAGRAM_SESSIONID?: string;
    INSTAGRAM_CSRFTOKEN?: string;
    INSTAGRAM_DS_USER_ID?: string;
    INSTAGRAM_MID?: string;
    INSTAGRAM_IG_DID?: string;
    INSTAGRAM_RUR?: string;
    /** Secret: the FCM service-account JSON. Unset, push is a no-op. */
    FCM_SERVICE_ACCOUNT?: string;
    /**
     * Secret: HMAC key shared with the house egress proxy. Unset (local dev,
     * Worker Previews), TikTok lookups go direct only.
     */
    EGRESS_PROXY_SECRET?: string;
    /**
     * Secret: signs /api/proxy-media URLs and media passes. Optional: unset,
     * a key derived from REALTIME_TICKET_SECRET is used; neither set, the
     * generic media proxy is off (server/media-proxy/signing.ts).
     */
    MEDIA_PROXY_SECRET?: string;
    /** Egress proxy base URL; defaults to https://egress.115jon.com. */
    EGRESS_PROXY_URL?: string;
    /** Admin user ids ("id1, id2"): manual badges (POST /api/admin/badges). */
    COLLECTIBLES_ADMIN_USER_IDS?: string;
    /**
     * Billing (not set up yet). While any of the three is unset,
     * /api/billing/* answers 501 (services/billing.service.ts).
     */
    STRIPE_SECRET_KEY?: string;
    STRIPE_WEBHOOK_SECRET?: string;
    /** The Stripe price id for Yap Loud. */
    STRIPE_PRICE_LOUD?: string;
  }

  namespace Cloudflare {
    interface Env extends CloudflareEnv {}
  }

  // ── Chromium Insertable Streams (WebRTC Breakout Box) ───────────────────
  // These APIs are Chromium-only (Chrome 94+, Edge 94+).
  // Used to bypass Chrome's APM mono downmix for true stereo mic input.

  interface MediaStreamTrackProcessorInit {
    track: MediaStreamTrack;
  }

  var MediaStreamTrackProcessor:
    | {
        prototype: MediaStreamTrackProcessor;
        new (init: MediaStreamTrackProcessorInit): MediaStreamTrackProcessor;
      }
    | undefined;

  interface MediaStreamTrackProcessor {
    readonly readable: ReadableStream;
  }

  interface MediaStreamTrackGeneratorInit {
    kind: "audio" | "video";
  }

  var MediaStreamTrackGenerator:
    | {
        prototype: MediaStreamTrackGenerator;
        new (init: MediaStreamTrackGeneratorInit): MediaStreamTrackGenerator;
      }
    | undefined;

  interface MediaStreamTrackGenerator extends MediaStreamTrack {
    readonly writable: WritableStream;
  }
}

export {};

declare module "*.css?url" {
  const src: string;
  export default src;
}
