/**
 * What the public boot bundle (src/boot) and the gated console bundle
 * (src/console/app.tsx) agree on. Types only: nothing here is bundled, so
 * the boot bundle learns nothing about the console from it.
 */

/** GET /api/session for an admin. */
export interface AdminSessionInfo {
  user_id: string;
  /**
   * The console bundle: its entry module and stylesheets. Only an admin's
   * session gets these paths, and only an admin's session can load them.
   */
  console?: { script: string; styles: string[] };
}

export interface ConsoleAppProps {
  adminId: string;
  /** False while the boot page gets a new session: queries hold, no polls. */
  active: boolean;
  /** Bumps each time a session is (re)established; the console refetches. */
  epoch: number;
  /** A request found the session gone (GET /api/session bare-404s too). */
  onSessionLost: () => void;
  /** The admin asked to sign out. */
  onSignOut: () => void;
}

/** The console bundle's default export. */
export type ConsoleModule = {
  default: (props: ConsoleAppProps) => React.ReactNode;
};
