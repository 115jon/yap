import fs from "node:fs";
import path from "path";
import { defineConfig, type Plugin } from "vitest/config";

/**
 * `.wasm?module` as the Cloudflare Vite plugin gives it to the Worker: a
 * compiled WebAssembly.Module (src/lib/yap-core-wasm/server.ts).
 */
function wasmModuleImports(): Plugin {
  const suffix = ".wasm?module";
  return {
    name: "wasm-module-imports",
    enforce: "pre",
    async resolveId(source, importer) {
      if (!source.endsWith(suffix)) return null;
      const file = source.slice(0, -"?module".length);
      const resolved = await this.resolve(file, importer, { skipSelf: true });
      return resolved ? `${resolved.id}?module` : null;
    },
    load(id) {
      if (!id.endsWith(suffix)) return null;
      const file = id.slice(0, -"?module".length);
      if (!fs.existsSync(file)) {
        throw new Error(`${file} is missing: run pnpm core-wasm:build first.`);
      }
      return `import fs from "node:fs";
export default new WebAssembly.Module(fs.readFileSync(${JSON.stringify(file)}));`;
    },
  };
}

export default defineConfig({
  plugins: [wasmModuleImports()],
  test: {
    globals: true,
    environment: "node",
    setupFiles: ["./src/test/setup-dom.ts", "./src/test/setup-core-wasm.ts"],
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
