/**
 * Pieces every widget uses: the props they take, the plate wrapper that
 * knows about drafts, the quiet empty state, and still-at-rest images.
 */

import {
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";

import { Icon, type IconName } from "@/components/ui/icon";
import { useAppActive } from "@/hooks/useAppActive";
import type { BoardData, ProfileWidget, WidgetType } from "@/lib/profile-board";
import { cn } from "@/lib/utils";

import type { BoardCtx } from "../board-format";
import { WidgetPlate, type WidgetPlateProps } from "../WidgetPlate";

export interface WidgetProps<T extends WidgetType = WidgetType> {
  widget: ProfileWidget<T>;
  data: BoardData;
  ctx: BoardCtx;
}

/** WidgetPlate for `widget`, with the draft tag when the owner's plan lacks it. */
export function Plate({
  widget,
  ctx,
  ...props
}: Omit<WidgetPlateProps, "type" | "size"> & {
  widget: ProfileWidget;
  ctx: BoardCtx;
}) {
  return (
    <WidgetPlate
      type={widget.type}
      size={widget.size}
      draft={ctx.isOwner && ctx.drafts.has(widget.id)}
      {...props}
    />
  );
}

/**
 * A widget with nothing to show: a quiet glyph, one line and a hint. No
 * art and no action: the board stays calm, and the owner edits in place.
 */
export function WidgetEmpty({
  icon,
  title,
  hint,
  className,
}: {
  icon: IconName;
  title: ReactNode;
  hint?: ReactNode;
  className?: string;
}) {
  return (
    <div
      data-slot="widget-empty"
      className={cn(
        "flex min-h-0 flex-1 flex-col justify-center gap-1",
        className,
      )}
    >
      <Icon name={icon} size="lg" className="mb-1 text-ink-3" />
      <p className="text-ui-sm font-medium text-pretty text-ink-2">{title}</p>
      {hint ? (
        <p className="text-ui-xs text-pretty text-ink-3">{hint}</p>
      ) : null}
    </div>
  );
}

/** Bricolage 700 title on a plate. */
export function WidgetTitle({
  className,
  children,
  title,
}: {
  className?: string;
  children: ReactNode;
  title?: string;
}) {
  return (
    <p
      title={title}
      className={cn(
        "font-display text-[16px] leading-5 font-bold tracking-[-0.01em] text-ink-1",
        className,
      )}
    >
      {children}
    </p>
  );
}

// ── Reduced motion ────────────────────────────────────────────────────────

const REDUCED = "(prefers-reduced-motion: reduce)";

function subscribeReduced(onChange: () => void) {
  if (typeof window === "undefined" || !window.matchMedia) return () => {};
  const query = window.matchMedia(REDUCED);
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

export function usePrefersReducedMotion(): boolean {
  return useSyncExternalStore(
    subscribeReduced,
    () =>
      typeof window !== "undefined" && !!window.matchMedia?.(REDUCED).matches,
    () => false,
  );
}

// ── Images ────────────────────────────────────────────────────────────────

/** GIFs are the only widget images that move (WebP uploads are re-encoded). */
export function isGifUrl(src: string): boolean {
  return /\.gif(?:$|[?#])/i.test(src);
}

/**
 * A user image cropped into its box (the caller sizes the box). Lazy, and
 * a GIF holds its first frame until hovered or focused.
 */
export function WidgetImage({
  src,
  alt,
  className,
  gif = isGifUrl(src),
}: {
  src: string;
  alt: string;
  className?: string;
  gif?: boolean;
}) {
  const [failed, setFailed] = useState<string | null>(null);
  const box = cn(
    "relative block overflow-hidden rounded-[2px] bg-ply-4",
    className,
  );
  if (failed === src) {
    return (
      <span className={cn(box, "flex items-center justify-center")}>
        <Icon name="imageBroken" size="lg" className="text-ink-3" />
        <span className="sr-only">{alt || "Image didn't load"}</span>
      </span>
    );
  }
  if (gif) {
    return (
      <StillGif
        src={src}
        alt={alt}
        className={box}
        onError={() => setFailed(src)}
      />
    );
  }
  return (
    <span className={box}>
      <img
        src={src}
        alt={alt}
        loading="lazy"
        decoding="async"
        draggable={false}
        onError={() => setFailed(src)}
        className="absolute inset-0 size-full object-cover"
      />
    </span>
  );
}

/**
 * A GIF painted once to a canvas (its first frame) when it scrolls into
 * view; the moving image mounts only while hovered or focused, the app is
 * active and motion is allowed. Nothing runs at rest.
 */
function StillGif({
  src,
  alt,
  className,
  onError,
}: {
  src: string;
  alt: string;
  className: string;
  onError(): void;
}) {
  const hostRef = useRef<HTMLSpanElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [hot, setHot] = useState(false);
  const active = useAppActive();
  const reduced = usePrefersReducedMotion();
  const errorRef = useRef(onError);
  useEffect(() => {
    errorRef.current = onError;
  });

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let cancelled = false;
    const paint = () => {
      const image = new Image();
      image.decoding = "async";
      image.onload = () => {
        const canvas = canvasRef.current;
        if (cancelled || !canvas) return;
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        canvas.getContext("2d")?.drawImage(image, 0, 0);
      };
      image.onerror = () => {
        if (!cancelled) errorRef.current();
      };
      image.src = src;
    };
    if (typeof IntersectionObserver === "undefined") {
      paint();
      return () => {
        cancelled = true;
      };
    }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        observer.disconnect();
        paint();
      }
    });
    observer.observe(host);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [src]);

  const playing = hot && active && !reduced;
  return (
    <span
      ref={hostRef}
      role="img"
      aria-label={alt || "Animated image"}
      tabIndex={0}
      data-playing={playing || undefined}
      className={cn(
        className,
        "outline-none focus-visible:ring-2 focus-visible:ring-accent-ink",
      )}
      onPointerEnter={() => setHot(true)}
      onPointerLeave={() => setHot(false)}
      onFocus={() => setHot(true)}
      onBlur={() => setHot(false)}
    >
      <canvas
        ref={canvasRef}
        aria-hidden="true"
        className="absolute inset-0 size-full object-cover"
      />
      {playing ? (
        <img
          src={src}
          alt=""
          aria-hidden="true"
          draggable={false}
          className="absolute inset-0 size-full object-cover"
        />
      ) : (
        <span className="silkscreen pointer-events-none absolute bottom-1.5 left-1.5 rounded-[2px] bg-ply-0/80 px-1 text-ink-1">
          GIF
        </span>
      )}
    </span>
  );
}
