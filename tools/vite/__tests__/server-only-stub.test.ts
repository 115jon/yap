import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { build, type Rollup } from "vite";
import { afterEach, describe, expect, it } from "vitest";
import { serverOnlyStub } from "../server-only-stub";

let root: string | null = null;

afterEach(async () => {
  if (root) await rm(root, { recursive: true, force: true });
  root = null;
});

async function project(files: Record<string, string>): Promise<string> {
  root = await mkdtemp(path.join(tmpdir(), "server-only-stub-"));
  for (const [name, code] of Object.entries(files)) {
    const file = path.join(root, name);
    await mkdir(path.dirname(file), { recursive: true });
    await writeFile(file, code);
  }
  return root;
}

async function bundle(dir: string, stubbed: boolean): Promise<string> {
  const result = (await build({
    root: dir,
    logLevel: "silent",
    configFile: false,
    plugins: stubbed ? [serverOnlyStub([path.join(dir, "server")])] : [],
    build: {
      write: false,
      minify: false,
      lib: { entry: path.join(dir, "main.js"), formats: ["es"] },
    },
  })) as Rollup.RollupOutput[];
  return result
    .flatMap((output) => output.output)
    .map((chunk) => ("code" in chunk ? chunk.code : ""))
    .join("\n");
}

// Mirrors a desktop route: a server-fn body (never called in the SPA)
// dynamically imports a service that statically imports a server module.
const FILES = {
  "main.js": `
    export const handler = async () => {
      const { deliver } = await import("./service.js");
      return deliver("x");
    };
    export const shown = "client";
  `,
  "service.js": `
    import { proxied } from "./server/rewrite.js";
    export const deliver = (value) => proxied(value);
  `,
  "server/rewrite.js": `
    import { AsyncLocalStorage } from "node:async_hooks";
    const storage = new AsyncLocalStorage();
    export const proxied = (value) => storage.getStore() ?? value + "-SERVER-CODE";
  `,
};

describe("serverOnlyStub", () => {
  it("fails without the stub, as the desktop build did", async () => {
    const dir = await project(FILES);
    await expect(bundle(dir, false)).rejects.toThrow(/AsyncLocalStorage/);
  });

  it("keeps server modules and their Node imports out of the bundle", async () => {
    const dir = await project(FILES);
    const code = await bundle(dir, true);
    expect(code).toContain('"client"');
    expect(code).not.toContain("SERVER-CODE");
    expect(code).not.toContain("AsyncLocalStorage");
  });

  it("leaves modules outside the listed directories alone", async () => {
    const dir = await project(FILES);
    const code = await bundle(dir, true);
    expect(code).toContain("const deliver");
  });
});
