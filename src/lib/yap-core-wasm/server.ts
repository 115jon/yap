// yap-core-wasm in the Worker: the Cloudflare Vite plugin imports
// `.wasm?module` as a compiled WebAssembly.Module (workerd has no
// streaming compile and cannot fetch its own assets), and it is
// instantiated as this module loads, so every export is ready for the
// request that imported it.

import { initSync } from "./pkg/yap_core_wasm.js";
import wasmModule from "./pkg/yap_core_wasm_bg.wasm?module";

initSync({ module: wasmModule });

export * from "./index";
