// Signed /api/proxy-media paths the server handed us, by the original URL
// inside them. The app unwraps embed media to their originals (provider
// detection keeps working on real hosts); `getAuthAssetUrl` puts the
// server's signature back when the media loads, so it loads without the
// media pass too (public share pages, the first moments after sign-in).
// No imports: platform.ts and proxy-media-url.ts both use it.

const LIMIT = 2_000;
const paths = new Map<string, string>();

export function rememberSignedMediaPath(rawUrl: string, path: string): void {
  if (paths.has(rawUrl)) paths.delete(rawUrl);
  paths.set(rawUrl, path);
  if (paths.size > LIMIT) {
    const oldest = paths.keys().next().value;
    if (oldest !== undefined) paths.delete(oldest);
  }
}

export function signedMediaPathFor(rawUrl: string): string | null {
  return paths.get(rawUrl) ?? null;
}
