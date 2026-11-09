/**
 * The providers every console mount shares (the real app and fixtures):
 * Yap's theme switcher, tooltips and toasts.
 */

import { ThemeProvider } from "next-themes";
import type { ReactNode } from "react";

import { Toaster } from "@/components/ui/toast";
import { TooltipProvider } from "@/components/ui/tooltip";
import { NEXT_THEME_NAMES } from "@/lib/theme-preferences";

export function ConsoleShell({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider
      attribute="class"
      defaultTheme="system"
      enableSystem
      disableTransitionOnChange
      themes={NEXT_THEME_NAMES}
    >
      <TooltipProvider>
        {children}
        <Toaster />
      </TooltipProvider>
    </ThemeProvider>
  );
}
