// Loads yap-core-wasm before any test, as the app does before the panels
// that use it render (chat/lazy-panels.ts) and the Worker does on import
// (yap-core-wasm/server.ts). Build it first: pnpm core-wasm:build.
import "@/lib/yap-core-wasm/server";
