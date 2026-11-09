import {
  displayNameStyleHasMotion,
  getDisplayNameEffectOverlayStyle,
  getDisplayNameEffectStyle,
  getDisplayNameFontStyle,
  isLightDisplayColor,
  isYapDisplayNameEffect,
  normalizeDisplayColor,
  normalizeDisplayNameStyle,
  resolveReadableDisplayNameStyle,
  type DisplayNameStyle,
} from "@/lib/profile-customization";
import {
  DITHER_FADE_ROWS,
  ditherFadeColumns,
  getDitherFadeImage,
} from "@/lib/name-dither";
import { cn } from "@/lib/utils";
import { useCallback, useMemo, useState, type CSSProperties } from "react";
import InlineEmojiText from "./InlineEmojiText";
import {
  useDecorationPlayback,
  type DecorationPlayback,
} from "./ProfileAssetLayer";

interface ProfileDisplayNameProps {
  text: string;
  displayNameStyle?: DisplayNameStyle | string | null;
  className?: string;
  style?: CSSProperties;
  backgroundColor?: string | null;
  readableFallbackColor?: string | null;
  minContrastRatio?: number;
  /**
   * When the effect plays. `"hover"` (default): static, plays while the
   * row holds the pointer or focus. `true`: plays while mounted (an open
   * profile surface). `false`: always static.
   */
  animate?: DecorationPlayback;
}

const NBSP = "\u00a0";
const CUSTOM_EMOJI_RE = /<:[a-z0-9_]+:[a-z0-9-]+>/i;
const PICTOGRAPHIC_RE = /\p{Extended_Pictographic}/u;
/** Longer names are not split (Wave falls back to one bob). */
const MAX_SPLIT = 48;

let graphemeSegmenter: Intl.Segmenter | null | undefined;

function graphemes(text: string): string[] {
  if (graphemeSegmenter === undefined) {
    graphemeSegmenter =
      typeof Intl !== "undefined" && "Segmenter" in Intl
        ? new Intl.Segmenter(undefined, { granularity: "grapheme" })
        : null;
  }
  return graphemeSegmenter
    ? Array.from(graphemeSegmenter.segment(text), (part) => part.segment)
    : Array.from(text);
}

/**
 * The name as one string per character, or null when it cannot be split
 * (it holds custom or native emoji, which render as images, or it is very
 * long). Only Wave and Glitch split a name.
 */
function splitName(text: string): string[] | null {
  if (CUSTOM_EMOJI_RE.test(text) || PICTOGRAPHIC_RE.test(text)) return null;
  const parts = graphemes(text);
  return parts.length > 0 && parts.length <= MAX_SPLIT ? parts : null;
}

export function ProfileDisplayName({
  text,
  displayNameStyle,
  className,
  style,
  backgroundColor,
  readableFallbackColor,
  minContrastRatio,
  animate = "hover",
}: ProfileDisplayNameProps) {
  const [spanNode, setSpanNode] = useState<HTMLSpanElement | null>(null);
  const normalizedStyle = useMemo(
    () => normalizeDisplayNameStyle(displayNameStyle),
    [displayNameStyle],
  );
  const explicitContrastContext = useMemo(
    () => ({
      backgroundColor: normalizeDisplayColor(backgroundColor),
      fallbackTextColor: normalizeDisplayColor(readableFallbackColor),
    }),
    [backgroundColor, readableFallbackColor],
  );
  const handleSpanRef = useCallback((node: HTMLSpanElement | null) => {
    setSpanNode(node);
  }, []);
  const contrastContext = useMemo(() => {
    if (
      !normalizedStyle ||
      explicitContrastContext.backgroundColor ||
      !spanNode ||
      typeof window === "undefined"
    ) {
      return explicitContrastContext;
    }

    const fallbackTextColor =
      explicitContrastContext.fallbackTextColor ??
      normalizeDisplayColor(window.getComputedStyle(spanNode).color);

    let resolvedBackgroundColor: string | null = null;
    let currentNode: HTMLElement | null = spanNode;
    while (currentNode) {
      const computedBackground = normalizeDisplayColor(
        window.getComputedStyle(currentNode).backgroundColor,
      );
      if (computedBackground) {
        resolvedBackgroundColor = computedBackground;
        break;
      }
      currentNode = currentNode.parentElement;
    }

    if (!resolvedBackgroundColor) {
      resolvedBackgroundColor = normalizeDisplayColor(
        window.getComputedStyle(document.body).backgroundColor,
      );
    }

    return {
      backgroundColor: resolvedBackgroundColor,
      fallbackTextColor,
    };
  }, [explicitContrastContext, normalizedStyle, spanNode]);

  const resolvedStyle = useMemo(
    () =>
      resolveReadableDisplayNameStyle(normalizedStyle, {
        backgroundColor: contrastContext.backgroundColor,
        fallbackTextColor: contrastContext.fallbackTextColor,
        minContrastRatio,
      }) ?? normalizedStyle,
    [contrastContext, minContrastRatio, normalizedStyle],
  );
  const hasMotion = resolvedStyle
    ? displayNameStyleHasMotion(resolvedStyle)
    : false;
  const playing = useDecorationPlayback(
    hasMotion ? spanNode : null,
    hasMotion ? animate : false,
  );
  const wave = resolvedStyle?.wave === true;
  const effect = resolvedStyle?.effect;
  const letters = useMemo(
    () => (wave || effect === "glitch" ? splitName(text) : null),
    [wave, effect, text],
  );

  if (!resolvedStyle) {
    return (
      <span ref={handleSpanRef} className={className} style={style}>
        <InlineEmojiText text={text} />
      </span>
    );
  }

  const yap = isYapDisplayNameEffect(resolvedStyle.effect);
  const length = letters?.length ?? graphemes(text).length;
  const effectStyle: CSSProperties & Record<string, string | number> = {
    ...getDisplayNameEffectStyle(resolvedStyle, { animate: playing }),
  };
  let letterFill: CSSProperties | undefined;
  if (letters && !yap && effectStyle.backgroundImage) {
    // A Classic fill moves to the glyphs: a transformed letter drops out
    // of its parent's `background-clip: text`. Size and offset are set per
    // letter in name-styles.css.
    effectStyle["--nm-fill"] = String(effectStyle.backgroundImage);
    delete effectStyle.backgroundImage;
    delete effectStyle.backgroundSize;
    effectStyle.WebkitTextFillColor = resolvedStyle.primaryColor;
    letterFill = { WebkitTextFillColor: "transparent" };
  }
  if (resolvedStyle.effect === "dither") {
    effectStyle["--nm-dither"] = getDitherFadeImage(
      resolvedStyle.primaryColor,
      resolvedStyle.secondaryColor,
      length,
    );
    // Tile height in ch, so its cells stay square.
    effectStyle["--nm-dr"] = (
      (DITHER_FADE_ROWS * length) /
      ditherFadeColumns(length)
    ).toFixed(3);
  }
  // A custom profile surface says which ground it is; otherwise the app
  // theme decides (name-styles.css).
  const ground = explicitContrastContext.backgroundColor
    ? isLightDisplayColor(explicitContrastContext.backgroundColor)
      ? "light"
      : "dark"
    : undefined;

  // Gradient and neon play on a second copy of the name that only fades
  // (compositor-only); it mounts while the effect plays and never in lists.
  const overlayStyle =
    playing && !letters
      ? getDisplayNameEffectOverlayStyle(resolvedStyle)
      : null;

  const content = letters ? (
    <>
      <span className="sr-only">{text}</span>
      <span aria-hidden="true" className="select-none">
        {letters.map((letter, index) => {
          // An inline-block space would collapse to nothing.
          const glyph = letter === " " ? NBSP : letter;
          // The letter moves (Wave); its glyph paints the fill. A
          // transformed box drops out of its parent's `background-clip:
          // text`, and an ellipsis takes the ink of the letter before it,
          // so the two jobs sit on two boxes.
          return (
            <span
              // Characters repeat; the index is the identity.
              key={index}
              className="rm-name-ch"
              data-ch={glyph}
              style={{ "--i": index } as CSSProperties}
            >
              <span className="rm-name-gl" style={letterFill}>
                {glyph}
              </span>
            </span>
          );
        })}
      </span>
    </>
  ) : (
    <InlineEmojiText text={text} />
  );

  return (
    <span
      ref={handleSpanRef}
      className={cn(
        // max-width comes from .rm-name (the parent's width plus the bleed).
        "rm-name inline-block align-top",
        overlayStyle && "relative",
        className,
      )}
      data-name-effect={resolvedStyle.effect}
      data-yap={yap ? "" : undefined}
      data-split={letters ? "" : undefined}
      data-wave={wave ? "" : undefined}
      data-name-play={playing && hasMotion ? "" : undefined}
      data-name-ground={ground}
      style={
        {
          ...getDisplayNameFontStyle(resolvedStyle.font),
          ...effectStyle,
          "--nm-n": length,
          ...style,
        } as CSSProperties
      }
    >
      {content}
      {overlayStyle ? (
        <span
          aria-hidden="true"
          data-display-name-overlay=""
          className="pointer-events-none absolute inset-0 overflow-hidden text-ellipsis select-none"
          style={{ padding: "inherit", ...overlayStyle }}
        >
          <InlineEmojiText text={text} />
        </span>
      ) : null}
    </span>
  );
}
