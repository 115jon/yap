import type { ReactNode } from "react";
import { RemoveScroll } from "react-remove-scroll";

/**
 * Lets a floating panel scroll while it is open over a Radix modal dialog.
 *
 * The dialog's scroll lock blocks wheel and touch scrolling outside its own
 * content, and floating panels are portaled outside it. Taking the top lock
 * (without isolation, so nothing else is blocked, and without touching the
 * scrollbar the dialog already hides) lets the panel's lists scroll again.
 * Away from a modal it renders its children as they are.
 */
export function OverModalScroll({
  overModal,
  children,
}: {
  overModal: boolean | undefined;
  children: ReactNode;
}) {
  if (!overModal) return children;
  return (
    <RemoveScroll noIsolation removeScrollBar={false} className="contents">
      {children}
    </RemoveScroll>
  );
}
