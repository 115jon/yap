// The browser's loader for yap-core-wasm: fetches and compiles the module
// once (about 26 KB brotli). Nothing at first paint needs it; the panels
// that do wait for it with their own chunk (chat/lazy-panels.ts).

import init from "./pkg/yap_core_wasm.js";
import wasmUrl from "./pkg/yap_core_wasm_bg.wasm?url";

let loading: Promise<void> | null = null;

/** Loads the module; later calls share the first. A no-op once loaded. */
export function loadYapCore(): Promise<void> {
  loading ??= init({ module_or_path: wasmUrl }).then(
    () => undefined,
    (error: unknown) => {
      // A later call tries again (a flaky network, an aborted fetch).
      loading = null;
      throw error;
    },
  );
  return loading;
}
