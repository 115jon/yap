import {
  PublicBody,
  PublicFrame,
  PublicTitle,
} from "@/components/auth/PublicFrame";
import {
  parsePolicy,
  type PolicyBlock,
  type PolicyInline,
} from "@/lib/policy-markdown";
import { createFileRoute } from "@tanstack/react-router";
import type { ReactNode } from "react";
// The policy's one source: the repository's PRIVACY.md, read at build time.
import privacyPolicy from "../../PRIVACY.md?raw";

const BLOCKS = parsePolicy(privacyPolicy);

/**
 * The privacy policy at /privacy (https://<public web host>/privacy, host
 * from config): PRIVACY.md in the public frame. The apps' Settings and the
 * Play listing link here; Play's account deletion link is
 * /privacy#deleting-your-account.
 */
export const Route = createFileRoute("/privacy")({
  component: PrivacyPage,
  head: () => ({
    meta: [
      { title: "Privacy policy — Yap" },
      {
        name: "description",
        content:
          "What Yap collects, where it goes, how long it is kept and how to delete it.",
      },
    ],
  }),
});

const LINK =
  "text-accent-ink underline decoration-line-strong underline-offset-[3px] [overflow-wrap:anywhere] pointer-fine:hover:decoration-accent-ink";

function Inlines({ inlines }: { inlines: PolicyInline[] }) {
  return inlines.map((inline, index) => {
    switch (inline.kind) {
      case "code":
        return (
          <code
            key={index}
            className="rounded-[2px] bg-tint-1 px-1 font-mono text-[0.9em] text-ink-1 [overflow-wrap:anywhere]"
          >
            {inline.text}
          </code>
        );
      case "strong":
        return (
          <strong key={index} className="font-semibold text-ink-1">
            {inline.text}
          </strong>
        );
      case "link":
        return (
          <a key={index} href={inline.href} className={LINK}>
            {inline.text}
          </a>
        );
      default:
        return inline.text;
    }
  });
}

function Block({ block }: { block: PolicyBlock }): ReactNode {
  if (block.kind === "heading") {
    if (block.level === 3) {
      return (
        <h3
          id={block.id}
          className="mt-8 scroll-mt-6 font-display text-ui-lg font-extrabold text-ink-1"
        >
          {block.text}
        </h3>
      );
    }
    return (
      <h2
        id={block.id}
        className="mt-12 scroll-mt-6 text-balance font-display text-[22px] leading-7 font-extrabold tracking-[-0.02em] text-ink-1"
      >
        {block.text}
      </h2>
    );
  }
  if (block.kind === "list") {
    return (
      <ul className="mt-4 flex list-disc flex-col gap-2 pl-5 marker:text-ink-3">
        {block.items.map((item, index) => (
          <li key={index} className="pl-1">
            <Inlines inlines={item} />
          </li>
        ))}
      </ul>
    );
  }
  return (
    <p className="mt-4">
      <Inlines inlines={block.inlines} />
    </p>
  );
}

// The title, then the line under it (the effective date), then the rest.
const TITLE = BLOCKS[0]?.kind === "heading" ? BLOCKS[0].text : "Privacy policy";
const AFTER_TITLE = BLOCKS[0]?.kind === "heading" ? BLOCKS.slice(1) : BLOCKS;
const LEAD = AFTER_TITLE[0]?.kind === "paragraph" ? AFTER_TITLE[0] : null;
const BODY = LEAD ? AFTER_TITLE.slice(1) : AFTER_TITLE;

function PrivacyPage() {
  return (
    <PublicFrame width="wide" label="Privacy">
      <PublicTitle size="display">{TITLE}</PublicTitle>
      {LEAD ? (
        <PublicBody size="lead">
          <Inlines inlines={LEAD.inlines} />
        </PublicBody>
      ) : null}
      <article className="mt-10 max-w-[68ch] text-pretty text-ui-base text-ink-2">
        {BODY.map((block, index) => (
          <Block key={index} block={block} />
        ))}
      </article>
    </PublicFrame>
  );
}
