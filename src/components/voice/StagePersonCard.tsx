import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";

import InlineEmojiText from "@/components/chat/InlineEmojiText";
import { useSpeakingAttr } from "@/hooks/useIsSpeaking";
import {
  fullSizeAvatarUrl,
  peekAmbientPicture,
  readAmbientPicture,
  WASH_H,
  WASH_W,
  type AmbientPicture,
} from "@/lib/ambient-color";
import { getAuthAssetUrl } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useProfileTint } from "./profile-tint";

/**
 * Where the card's light comes from:
 *   avatar   the picture's own colour (measured; the host allows CORS)
 *   accent   a picture whose pixels cannot be read: the theme accent
 *   tint     no picture: the person's profile tint
 *   pending  the picture is still loading: no glow yet
 */
export type AmbientSource = "avatar" | "accent" | "tint" | "pending";

/**
 * The picture's measured light: undefined while it loads, null when it cannot
 * be loaded at all.
 */
function useAmbientPicture(
  src: string | null,
): AmbientPicture | null | undefined {
  const [loaded, setLoaded] = useState<{
    src: string;
    picture: AmbientPicture | null;
  } | null>(null);

  useEffect(() => {
    if (!src) return;
    let live = true;
    // Resolves at once from the memo when another card measured it already.
    void readAmbientPicture(src).then((picture) => {
      if (live) setLoaded({ src, picture });
    });
    return () => {
      live = false;
    };
  }, [src]);

  if (!src) return null;
  if (loaded?.src === src) return loaded.picture;
  return peekAmbientPicture(src);
}

/**
 * The picture's 32x18 wash (ambient-color), copied onto this card's canvas
 * and stretched: drawn once per picture, so an animated avatar never repaints
 * it. Copying works even when the pixels cannot be read. The blur hides the
 * bilinear seams of the upscale; it is a user-image wash painted once, so it
 * opts out of the global no-blur rule.
 */
function Wash({ wash }: { wash: HTMLCanvasElement }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const ctx = ref.current?.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, WASH_W, WASH_H);
    ctx.drawImage(wash, 0, 0);
  }, [wash]);
  return (
    <canvas
      ref={ref}
      aria-hidden
      data-stage-wash=""
      data-rm-preserve-blur="true"
      width={WASH_W}
      height={WASH_H}
      className="absolute -inset-[10%] h-[120%] w-[120%] opacity-75 blur-lg"
    />
  );
}

/** The card's light: where it comes from and its colour. */
type AmbientLight = { source: AmbientSource; ambient: string };

/**
 * `hasPicture` is false without a picture (or when it failed to show);
 * `picture` is undefined while it loads and null when it cannot be measured.
 */
function ambientOf(
  hasPicture: boolean,
  picture: AmbientPicture | null | undefined,
  tint: string,
): AmbientLight {
  if (!hasPicture) return { source: "tint", ambient: tint };
  if (picture === undefined) return { source: "pending", ambient: tint };
  if (picture?.color) return { source: "avatar", ambient: picture.color };
  return { source: "accent", ambient: "var(--accent-ink)" };
}

export interface StagePersonCardProps {
  userId: string;
  name: string;
  /** The person's picture as stored (never a thumbnail rendition). */
  avatar?: string | null;
  /** Force the speaking ring (fixtures, tests); otherwise the speaking store. */
  speaking?: boolean;
  /** Overlays on the card (loading, camera unavailable, a hidden video). */
  children?: ReactNode;
  className?: string;
}

/**
 * A person without media on the focused stage, lit by their own picture
 * (YouTube's ambient mode; native/ FocusedStage.qml). A 16:9 card, or
 * 4:5 on a portrait stage (foundation.css `.rm-stage-person`):
 *
 *   glow   a soft shadow in the picture's colourful average (ambient-color),
 *          the accent when its pixels cannot be read, the profile tint
 *          without a picture
 *   wash   the same picture at 32x18, stretched and softened, under a scrim
 *          tinted with that colour so the face and name stand out
 *   face   the original picture (animated ones play), uncropped and without
 *          decorations as before, about 56% of the card's height (64-320px)
 *   ring   a 3px talk ring 6px out while they speak
 *
 * The parent must be a size container: the card fits its width and height.
 * Nothing runs per frame; the glow and scrim fade in once when the colour
 * arrives (instantly under reduced motion).
 */
export function StagePersonCard({
  userId,
  name,
  avatar,
  speaking,
  children,
  className,
}: StagePersonCardProps) {
  const src = avatar ? getAuthAssetUrl(fullSizeAvatarUrl(avatar)) : null;
  const [brokenSrc, setBrokenSrc] = useState<string | null>(null);
  const picture = useAmbientPicture(src);
  const tint = useProfileTint(userId);
  const subscribed = useSpeakingAttr(speaking === undefined ? userId : null);
  const speakingAttr =
    speaking === undefined
      ? subscribed
      : { "data-speaking": speaking ? ("" as const) : undefined };
  const showPicture = !!src && brokenSrc !== src;

  const { source, ambient } = ambientOf(showPicture, picture, tint);

  return (
    <div
      data-stage-person=""
      data-ambient={source}
      style={{ "--ambient": ambient } as CSSProperties}
      className={cn("rm-stage-person relative", className)}
    >
      <div
        aria-hidden
        data-stage-glow=""
        className={cn(
          "rm-stage-glow pointer-events-none absolute inset-0 rounded-lg transition-opacity duration-(--dur-3) ease-out motion-reduce:transition-none",
          source === "pending" ? "opacity-0" : "opacity-100",
        )}
      />
      <div className="absolute inset-0 overflow-hidden rounded-lg border border-line bg-ply-2">
        {showPicture && picture?.wash ? <Wash wash={picture.wash} /> : null}
        <div
          aria-hidden
          className="rm-stage-scrim absolute inset-0 transition-[background-color] duration-(--dur-3) ease-out motion-reduce:transition-none"
        />
      </div>

      <div className="relative flex h-full w-full flex-col items-center justify-center gap-[clamp(8px,4cqh,16px)] px-[10%]">
        <span
          {...speakingAttr}
          data-stage-face=""
          className="group/face relative block size-(--face) shrink-0 [--face:clamp(64px,min(56cqh,60cqw),320px)]"
        >
          <span
            aria-hidden
            data-stage-ring=""
            className="pointer-events-none absolute -inset-1.5 rounded-[4px] border-3 border-talk opacity-0 transition-opacity duration-(--dur-speak-off) ease-out group-data-speaking/face:opacity-100 group-data-speaking/face:duration-(--dur-speak-on)"
          />
          {showPicture ? (
            <img
              src={src}
              alt={name}
              decoding="async"
              draggable={false}
              onError={() => setBrokenSrc(src)}
              className="size-full rounded-avatar object-cover"
            />
          ) : (
            <span
              role="img"
              aria-label={name}
              className="flex size-full items-center justify-center rounded-avatar bg-tint-3 font-display [font-optical-sizing:auto] text-[calc(var(--face)*0.4)] leading-none font-extrabold text-ink-1 select-none"
            >
              {Array.from(name.trim())[0]?.toUpperCase() ?? "?"}
            </span>
          )}
        </span>
        <p className="w-full truncate text-center font-display [font-optical-sizing:auto] text-[18px] leading-6 font-bold text-ink-1 @min-[640px]:text-[24px] @min-[640px]:leading-8">
          <InlineEmojiText text={name} />
        </p>
      </div>
      {children}
    </div>
  );
}
