import { cloudflareTest } from "@cloudflare/vitest-plugin";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const sourceRoot = fileURLToPath(new URL("./src", import.meta.url));

export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.test.toml" },
    }),
  ],
  test: {
    include: [
      "realtime/__tests__/**/*.worker.test.ts",
      // The admin Worker (apps/admin) runs against the same D1 and DOs.
      "apps/admin/test/**/*.worker.test.ts",
    ],
    setupFiles: ["./realtime/test-setup.ts"],
    // The Durable Objects import Effect Schema (src/lib/gateway-protocol).
    // Pre-bundled, it loads as one module instead of hundreds per test file.
    deps: { optimizer: { ssr: { enabled: true, include: ["effect"] } } },
  },
  resolve: {
    alias: {
      "@": path.resolve(sourceRoot),
      // Route handlers run without a TanStack Start request (see the shim).
      "@tanstack/react-start/server": fileURLToPath(
        new URL("./realtime/test-shims/react-start-server.ts", import.meta.url),
      ),
    },
  },
});
