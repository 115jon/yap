/** `vite --mode fixtures`: the fixture page in the console's providers. */

import { StrictMode } from "react";
import type { Root } from "react-dom/client";

import { FixturesPage } from "./fixtures";
import { ConsoleShell } from "./shell";

export function renderFixtures(root: Root): void {
  root.render(
    <StrictMode>
      <ConsoleShell>
        <FixturesPage />
      </ConsoleShell>
    </StrictMode>,
  );
}
