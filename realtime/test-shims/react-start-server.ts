// Worker tests call API route handlers directly, outside a TanStack Start
// request. Routes that read the current request's headers (requireAuth()
// with no argument) read the ones a test set here; none by default.
let current: Headers | null = null;

/** The "current request" headers for the next handler call (null clears). */
export function setTestRequestHeaders(headers: HeadersInit | null): void {
  current = headers ? new Headers(headers) : null;
}

export function getRequestHeader(name: string): string | undefined {
  return current?.get(name) ?? undefined;
}
