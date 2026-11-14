/**
 * Local dev-server ports (`vite dev` only).
 *
 * The web dev server (root vite.config.ts) listens on 5173 unless
 * `RALPH_DEV_PORT` says otherwise, with `strictPort`. The "Yap (dev)" Kova
 * app only accepts http://localhost:5173 and http://127.0.0.1:5173 as
 * origins, so a server that quietly moved to the next free port would load
 * fine and then fail every sign-in.
 *
 * Pure, so the parsing and messages are unit tested
 * (`__tests__/dev-port.test.ts`).
 */

export const DEFAULT_WEB_DEV_PORT = 5173;

/** `RALPH_DEV_PORT` to a port: unset or blank is 5173; anything invalid throws. */
export function resolveWebDevPort(raw: string | undefined): number {
  const value = (raw ?? "").trim();
  if (value === "") return DEFAULT_WEB_DEV_PORT;
  const port = Number(value);
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(
      `RALPH_DEV_PORT must be a port number from 1 to 65535 (got ${JSON.stringify(raw)}).`,
    );
  }
  return port;
}

/** The startup error when a dev server's port is taken. */
export function describeBusyDevPort(server: "web", port: number): string {
  const next = port === 65535 ? port - 1 : port + 1;
  const signIn =
    port === DEFAULT_WEB_DEV_PORT
      ? `Kova sign-in only works on ${DEFAULT_WEB_DEV_PORT}, so on another port ` +
        `the app loads but sign-in fails.`
      : `Kova sign-in only works on ${DEFAULT_WEB_DEV_PORT}.`;
  return (
    `[ralph] Port ${port} is already in use. Stop the process using it, or ` +
    `pick another port with RALPH_DEV_PORT=${next} pnpm dev. ${signIn}`
  );
}
