/**
 * The admin console's client build: two pages, the public sign-in
 * (index.html) and the console (console.html), into dist/public, with a
 * manifest the Worker reads to know which files the sign-in page may load.
 * The Worker itself is bundled by Wrangler (wrangler.toml).
 *
 * The console imports Yap's own components, hooks, tokens and stylesheet
 * through the same `@/` alias the app uses (../../src); nothing is copied.
 *
 *   vite --config apps/admin/vite.config.ts                  the console, served
 *                                                             without the Worker
 *   vite --config apps/admin/vite.config.ts --mode fixtures  mock data
 *   vite build --config apps/admin/vite.config.ts            dist/public
 */

import tailwindcss from "@tailwindcss/vite";
import viteReact from "@vitejs/plugin-react";
import path from "node:path";
import { readdirSync, readFileSync } from "node:fs";
import { defineConfig, type Plugin } from "vite";

const here = import.meta.dirname;
const appSrc = path.resolve(here, "../../src");
const shims = path.resolve(appSrc, "shims");

/**
 * Yap's icons (public/icons: the home mark and each theme's Yappy), served
 * in dev and emitted into the build at the same /icons/ paths the shared
 * components use. Only this folder, not Yap's whole public/.
 */
function yapIcons(): Plugin {
  const dir = path.resolve(here, "../../public/icons");
  return {
    name: "yap-admin-icons",
    configureServer(server) {
      server.middlewares.use("/icons", (req, res, next) => {
        const name = path.basename(
          decodeURIComponent((req.url ?? "").split("?")[0]!),
        );
        try {
          const body = readFileSync(path.join(dir, name));
          res.setHeader(
            "Content-Type",
            name.endsWith(".svg") ? "image/svg+xml" : "image/png",
          );
          res.end(body);
        } catch {
          next();
        }
      });
    },
    generateBundle() {
      for (const name of readdirSync(dir)) {
        this.emitFile({
          type: "asset",
          fileName: `icons/${name}`,
          source: readFileSync(path.join(dir, name)),
        });
      }
    },
  };
}

export default defineConfig(({ mode }) => ({
  root: here,
  // Nothing of Yap's public/ beyond the icons (yapIcons).
  publicDir: false,
  // Dev's pre-bundled deps live with the root's, out of lint's way.
  cacheDir: path.resolve(here, "../../node_modules/.vite/admin"),
  plugins: [tailwindcss(), viteReact(), yapIcons()],
  resolve: {
    alias: {
      "@": appSrc,
      // Yap's modules are written for the web app; these are its client shims.
      "cloudflare:workers": path.resolve(shims, "cloudflare-workers.ts"),
      "@tauri-apps/plugin-shell": path.resolve(shims, "tauri-plugin-shell.ts"),
      "@tauri-apps/plugin-updater": path.resolve(
        shims,
        "tauri-plugin-updater.ts",
      ),
      "@tauri-apps/plugin-process": path.resolve(
        shims,
        "tauri-plugin-process.ts",
      ),
      "@tauri-apps/plugin-autostart": path.resolve(
        shims,
        "tauri-plugin-autostart.ts",
      ),
      "@tauri-apps/plugin-notification": path.resolve(
        shims,
        "tauri-plugin-notification.ts",
      ),
      "tauri-plugin-status-bar-color-api": path.resolve(
        shims,
        "tauri-plugin-status-bar-color-api.ts",
      ),
    },
  },
  define: {
    __IS_DESKTOP__: false,
    __IS_MOBILE__: false,
  },
  server: {
    port: Number(process.env.ADMIN_DEV_PORT ?? 5260),
    strictPort: true,
  },
  build: {
    outDir: "dist/public",
    emptyOutDir: true,
    manifest: "manifest.json",
    sourcemap: false,
    rollupOptions: {
      input: {
        signin: path.resolve(here, "index.html"),
        console: path.resolve(here, "console.html"),
      },
      // Hash-only names: the public sign-in files say nothing about the app.
      output: {
        entryFileNames: "assets/[hash].js",
        chunkFileNames: "assets/[hash].js",
        assetFileNames: "assets/[hash][extname]",
      },
    },
  },
  mode,
}));
