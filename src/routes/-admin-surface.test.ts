import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// The site-admin console is its own Worker (apps/admin). The main Worker
// must carry no admin surface: no /admin page and no /api/admin route but
// the collectibles-era POST /api/admin/badges, which predates the console
// and keeps its own COLLECTIBLES_ADMIN_USER_IDS gate.

const routesDir = path.resolve(__dirname);
const tree = readFileSync(path.join(routesDir, "../routeTree.gen.ts"), "utf8");

function routeFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return routeFiles(full);
    return /\.(ts|tsx)$/.test(entry.name) && !entry.name.startsWith("-")
      ? [path.relative(routesDir, full).replaceAll("\\", "/")]
      : [];
  });
}

describe("the main Worker's routes", () => {
  it("have no admin console page or admin API", () => {
    const paths = [...tree.matchAll(/path: '([^']+)'/g)].map(
      (match) => match[1]!,
    );
    const fullPaths = [...tree.matchAll(/fullPath: '([^']+)'/g)].map(
      (match) => match[1]!,
    );
    const admin = [...new Set([...paths, ...fullPaths])].filter((route) =>
      /(^|\/)admin(\/|$)/.test(route),
    );
    expect(admin).toEqual(["/api/admin/badges"]);
  });

  it("have no admin route files", () => {
    const admin = routeFiles(routesDir).filter((file) =>
      /(^|\/)admin(\/|\.|$)/.test(file),
    );
    expect(admin).toEqual(["api/admin/badges.ts"]);
  });

  it("tell no one whether they are a site admin", () => {
    const me = readFileSync(path.join(routesDir, "api/users/me.ts"), "utf8");
    expect(me).not.toMatch(/site_admin|SITE_ADMIN/);
  });
});
