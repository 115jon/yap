import path from "node:path";
import type { Plugin } from "vite";

const STUB_PREFIX = "\0server-only-stub:";

/**
 * Replaces server-only modules with an empty stub in a client build.
 *
 * The desktop SPA build has no TanStack Start plugin, so a route's
 * `createServerFn` handler is never stripped: its body is dead (the shim
 * never calls it), but Rollup still bundles what it imports. A server module
 * that needs Node built-ins (`node:async_hooks`, `node:crypto`) then fails
 * the build, and any other one ships server code to the client.
 *
 * Every import that resolves into one of `dirs` becomes a module whose named
 * exports are all `undefined` (Rollup's `syntheticNamedExports`), so the
 * import links and the server code stays out. Calling a stubbed export
 * throws a TypeError, which is only reachable from server-only code paths.
 */
export function serverOnlyStub(dirs: string[]): Plugin {
  const roots = dirs.map((dir) => normalize(path.resolve(dir)) + "/");
  const isServerOnly = (id: string) => {
    const file = normalize(id.split("?")[0]);
    return roots.some((root) => file.startsWith(root));
  };

  return {
    name: "yap:server-only-stub",
    apply: "build",
    enforce: "pre",
    async resolveId(source, importer, options) {
      if (source.startsWith(STUB_PREFIX)) return source;
      const resolved = await this.resolve(source, importer, {
        ...options,
        skipSelf: true,
      });
      if (!resolved || resolved.external || !isServerOnly(resolved.id)) {
        return null;
      }
      return {
        id: `${STUB_PREFIX}${normalize(resolved.id)}`,
        syntheticNamedExports: true,
      };
    },
    load(id) {
      if (!id.startsWith(STUB_PREFIX)) return null;
      return {
        code: "export default {};\n",
        syntheticNamedExports: true,
        moduleSideEffects: false,
      };
    },
  };
}

function normalize(file: string): string {
  return file.replace(/\\/g, "/").toLowerCase();
}
