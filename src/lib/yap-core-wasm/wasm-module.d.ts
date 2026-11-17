// `.wasm?module` imports (the Cloudflare Vite plugin, and the stand-in in
// vitest.config.ts): a compiled module.
declare module "*.wasm?module" {
  const module: WebAssembly.Module;
  export default module;
}
