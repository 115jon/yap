/**
 * The admin page's one component: it runs the auth machine
 * (./auth-machine.ts) and, once there is an admin session, loads the gated
 * console bundle and mounts it in place. Sign-in to console never reloads
 * the page:
 *
 *   checking    GET /api/session: is there an admin cookie already?
 *   signed-out  Kova's SignIn (neutral: no app wording or code)
 *   verifying   the Kova app token, once, to POST /api/session
 *   admin       the console (its stylesheets, then its module)
 *   denied      the Kova session is signed out; the neutral page says so
 *
 * When the session ends mid-use the console stays mounted, on its route,
 * under a sign-in prompt; the prompt verifies on its own when Kova is still
 * signed in.
 */

import { SignIn, useAuth } from "@kova/react";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type ReactNode,
} from "react";

import type { ConsoleAppProps } from "../shared/console-contract";
import { authReducer, initialAuthState, isConsolePath } from "./auth-machine";
import {
  checkCookie,
  clearCookie,
  exchangeToken,
  loadConsole,
  rememberReturnPath,
  takeReturnPath,
} from "./session";

/** A second try before a 404 counts as "not an admin" (a Kova blip). */
const REFUSAL_RECHECK_MS = 1_500;

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/**
 * Kova's signOut always navigates to its callback URL. A fragment-only URL
 * on the current page is a fragment navigation, not a reload, so the page
 * stays and the machine moves on; the fragment is then cleared.
 */
async function signOutInPlace(
  signOut: (url?: string) => Promise<void>,
  then = `${window.location.pathname}${window.location.search}`,
) {
  // Read the URL as late as possible: it must match the page's current URL
  // exactly (bar the fragment), or the navigation is a full load.
  const hashed = () => `${window.location.href.split("#")[0]}#signed-out`;
  await signOut(hashed()).catch(() => {});
  window.history.replaceState(null, "", then);
}

/** Open the console where the admin was before a redirect sign-in. */
function restoreReturnPath() {
  const path = takeReturnPath();
  if (path && window.location.pathname === "/" && isConsolePath(path)) {
    window.history.replaceState(null, "", path);
  }
}

export function Boot() {
  const { isLoaded, isSignedIn, getToken, signOut } = useAuth();
  const [state, dispatch] = useReducer(authReducer, initialAuthState);
  // The Kova session in view was refused or signed out: its "signed in" may
  // linger a moment (a stale session read, a sign-out still in flight), and
  // must not be verified again. A fresh sign-in comes after it reads false.
  const [staleKova, setStaleKova] = useState(false);
  if (staleKova && isLoaded && !isSignedIn) setStaleKova(false);
  const kovaSignedIn = isLoaded && isSignedIn && !staleKova;
  const verifying = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // checking: an admin cookie from an earlier visit opens the console at once.
  useEffect(() => {
    let cancelled = false;
    void checkCookie().then((result) => {
      if (cancelled) return;
      if (result.kind === "admin") {
        restoreReturnPath();
        dispatch({ type: "cookie-ok", session: result.session });
      } else {
        dispatch({ type: "cookie-missing" });
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // signed-out / denied: a Kova session (new, or still alive after the
  // cookie lapsed) goes to verifying on its own.
  useEffect(() => {
    if (state.phase !== "signed-out" && state.phase !== "denied") return;
    if (!kovaSignedIn || state.unreachable) return;
    dispatch({ type: "kova-signed-in" });
  }, [state.phase, state.unreachable, kovaSignedIn]);

  // verifying: one exchange per entry. Its own result moves the phase on, so
  // it is never cancelled by the state change it causes.
  useEffect(() => {
    if (state.phase !== "verifying" || verifying.current) return;
    verifying.current = true;
    void (async () => {
      try {
        const token = await getToken().catch(() => null);
        if (!token) {
          dispatch({ type: "unreachable" });
          return;
        }
        let result = await exchangeToken(token);
        if (result === "refused") {
          await sleep(REFUSAL_RECHECK_MS);
          result = await exchangeToken(token);
        }
        if (!mounted.current) return;
        if (result === "granted") {
          const cookie = await checkCookie();
          if (cookie.kind === "admin") {
            restoreReturnPath();
            dispatch({ type: "verified", session: cookie.session });
          } else {
            dispatch({ type: "unreachable" });
          }
          return;
        }
        if (result === "unreachable") {
          dispatch({ type: "unreachable" });
          return;
        }
        // Refused: sign the Kova session out so it is not left behind (and,
        // mid-use, so the prompt asks for a fresh sign-in).
        setStaleKova(true);
        dispatch({ type: "refused" });
        await signOutInPlace(signOut);
      } finally {
        verifying.current = false;
      }
    })();
  }, [state.phase, getToken, signOut]);

  // Lost mid-use: a hosted sign-in leaves the page, so remember the route.
  useEffect(() => {
    if (state.phase === "signed-out" && state.lost) {
      rememberReturnPath(
        `${window.location.pathname}${window.location.search}`,
      );
    }
  }, [state.phase, state.lost]);

  const onSessionLost = useCallback(
    () => dispatch({ type: "session-lost" }),
    [],
  );
  const onSignOut = useCallback(() => {
    void (async () => {
      setStaleKova(true);
      await clearCookie();
      dispatch({ type: "signed-out" });
      // Let the console (and its router) unmount before Kova navigates.
      await new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0)));
      await signOutInPlace(signOut, "/");
    })();
  }, [signOut]);

  const session = state.session;
  const script = session?.console?.script ?? "dev";
  const Console = useMemo(
    () => (session ? lazy(() => loadConsole(session)) : null),
    // One module per script path; a new session object is the same bundle.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [script, !!session],
  );

  if (!session || !Console) {
    if (state.phase === "signed-out" || state.phase === "denied") {
      return (
        <SignInCard
          refused={state.phase === "denied"}
          unreachable={state.unreachable}
          onRetry={() => dispatch({ type: "kova-signed-in" })}
          signedIn={kovaSignedIn}
        />
      );
    }
    return <Splash />;
  }

  const prompt = state.phase !== "admin";
  const props: ConsoleAppProps = {
    adminId: session.user_id,
    active: !prompt,
    epoch: state.epoch,
    onSessionLost,
    onSignOut,
  };
  return (
    <>
      <div className="boot-console" inert={prompt}>
        <Suspense fallback={<Splash />}>
          <Console {...props} />
        </Suspense>
      </div>
      {prompt ? (
        <div className="boot-overlay">
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="boot-prompt-title"
            className="boot-card"
          >
            {state.phase === "verifying" ||
            (kovaSignedIn && !state.unreachable) ? (
              <Splash inline label="Signing you back in" />
            ) : (
              <>
                <h1 id="boot-prompt-title">Your session ended</h1>
                <p className="boot-note">
                  Sign in again to pick up where you left off.
                </p>
                <SignInBody
                  unreachable={state.unreachable}
                  onRetry={() => dispatch({ type: "kova-signed-in" })}
                  signedIn={kovaSignedIn}
                />
              </>
            )}
          </div>
        </div>
      ) : null}
    </>
  );
}

function SignInCard({
  refused,
  ...body
}: {
  refused: boolean;
  unreachable: boolean;
  signedIn: boolean;
  onRetry: () => void;
}) {
  return (
    <main className="boot-page">
      <div className="boot-card">
        <h1>Sign in</h1>
        {refused ? (
          <p className="boot-note" role="alert">
            That account can&apos;t sign in here.
          </p>
        ) : null}
        <SignInBody {...body} />
      </div>
    </main>
  );
}

function SignInBody({
  unreachable,
  signedIn,
  onRetry,
}: {
  unreachable: boolean;
  signedIn: boolean;
  onRetry: () => void;
}): ReactNode {
  if (unreachable && signedIn) {
    return (
      <>
        <p className="boot-note" role="alert">
          Couldn&apos;t reach the server.
        </p>
        <button type="button" className="boot-button" onClick={onRetry}>
          Try again
        </button>
      </>
    );
  }
  return (
    <SignIn afterSignInUrl={`${window.location.origin}/`} signUpUrl={null} />
  );
}

/** A quiet loading mark: no words on screen, a name for screen readers. */
function Splash({
  inline = false,
  label = "Loading",
}: {
  inline?: boolean;
  label?: string;
}) {
  return (
    <div
      className={inline ? "boot-splash boot-splash-inline" : "boot-splash"}
      role="status"
      aria-busy="true"
    >
      <span className="boot-bar" aria-hidden="true" />
      <span className="boot-sr">{label}</span>
    </div>
  );
}
