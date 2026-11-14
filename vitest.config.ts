import path from "path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    setupFiles: ["./src/test/setup-dom.ts"],
    include: [
      "src/**/*.test.ts",
      "src/**/*.test.tsx",
      "realtime/**/*.test.ts",
      "packages/*/src/**/*.test.ts",
      "tools/vite/**/*.test.ts",
      // The admin console's client (its Worker tests run under
      // vitest.realtime.config.ts, from apps/admin/test).
      "apps/admin/src/**/*.test.ts",
      "apps/admin/src/**/*.test.tsx",
    ],
    exclude: ["node_modules", "realtime/__tests__/**/*.worker.test.ts"],
    coverage: {
      provider: "v8",
      include: [
        "src/lib/**/*.ts",
        "realtime/**/*.ts",
        "packages/*/src/**/*.ts",
      ],
      exclude: ["**/*.test.ts", "**/*.d.ts"],
    },
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "cloudflare:workers": path.resolve(
        __dirname,
        "./src/test/shims/cloudflare-workers.ts",
      ),
    },
  },
});
