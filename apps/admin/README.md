# yap-admin

The site-admin console for Yap, a separate Cloudflare Worker served only at a
private hostname. The hostname and the Kova key are never written into this
public repository, its issues or its pull requests. The main Worker (`ralph-meet`) has no admin
page or admin API.

It has five sections:

- **People:** every account with its plan, badges, stats and storage. Plans and manual badges are changed here, and sign-in can be suspended or live connections closed.
- **Live ops:** who is online, voice rooms, uploads in flight, D1 and R2 totals, the deploy, the shop catalog (last synced, items, last error, the fetcher's last push, and Refresh now) and cron runs.
- **Insights:** daily charts.
- **Switches:** kill switches, the site banner and the desktop native handoff (on/off and a rollout percent, off at 0 % by default; read by the Tauri app from `GET /api/desktop/native-handoff`).
- **Audit log:** every admin change.

## How it is put together

| Part                                         | Where                                        |
| -------------------------------------------- | -------------------------------------------- |
| Worker: the gate, the API and assets         | `src/server/worker.ts`, `src/server/gate.ts` |
| Reads and writes (D1, the audit log)         | `src/server/admin.service.ts`                |
| Live state and disconnects (Durable Objects) | `src/server/admin-gateway.service.ts`        |
| The page and its public boot bundle          | `index.html`, `src/boot/*`                   |
| The console bundle (admins only)             | `src/console/app.tsx`, `src/console/*`       |
| Routes                                       | `src/console/router.tsx`                     |
| Data (TanStack Query, the transport)         | `src/console/admin-data.ts`                  |
| Mock data for captures                       | `src/console/fixtures.tsx`                   |

- **One page, two bundles.** `index.html` loads only the boot bundle (`src/boot`): Kova sign-in and the auth state machine (`auth-machine.ts`: checking, signed-out, verifying, admin, denied). Once the session cookie is set, it imports the console bundle in place, with no reload. The Worker never serves the console bundle to anyone but an admin session, and only an admin's `GET /api/session` names its files.
- **Routes.** TanStack Router, code-based: `/people`, `/people/$userId`, `/live`, `/insights`, `/switches`, `/audit` (`/` goes to `/people`). The Worker serves the page at those paths to an admin session only, so deep links, refresh and back/forward work. Each section route has its own error boundary and loading skeleton.
- **Data.** TanStack Query. Each section polls only while the tab is visible (Live ops every 10 s, Switches and Audit every 30 s, People every minute, Insights every 5 minutes). Reads retry transient failures (network, 408, 429, 5xx) with backoff; writes retry the same failures in the transport, are optimistic where the section can roll back, and invalidate what they changed plus the audit log.
- **A session that ends mid-use.** A bare 404 from `/api/*` is re-checked against `GET /api/session` once before anything happens; the request is retried if the session is still good. Only when that check also bare-404s does the page ask for a sign-in, in place, over the console: the route and its state stay, and the console refetches once signed in again.

- **Shared code.** The console imports Yap's own components, hooks, tokens and stylesheet through the `@/` alias (`../../src`); nothing is copied. Both Vite (`vite.config.ts`) and Wrangler (`tsconfig.json` paths) resolve the alias.
- **Data.** It uses the main Worker's D1, KV and R2 (the same ids as the root `wrangler.toml`). It reaches the main Worker's Durable Objects through cross-script bindings (`script_name = "ralph-meet"`), and the main Worker itself through a service binding (`YAP`, for its `/api/healthz` build stamp). It uses internal stub paths only:
  - `MeetingRoom` `/admin/introspect` and `/admin/disconnect-user`
  - `VoiceRoom` `/disconnect-participant`
- **Writes the main Worker enforces.** The main Worker reads and enforces what this Worker writes:
  - `site_settings`: the switches, the banner and the native handoff (`nativeHandoff`), through a 10 s cache.
  - `users.suspended_at`: through a 15 s cache. The disconnect closes sockets at once.

## Who gets in

Kova is the only auth, using a dedicated Kova application, **Yap Admin**:

1. `GET /` with no admin session serves a neutral page (title "Sign in"). Its boot bundle loads Kova's `SignIn` and nothing else. The build gives files hash-only names, and the Worker serves only the files that bundle needs, read from the build manifest.
2. After sign-in, the page sends its Kova app token once to `POST /api/session`, with `Authorization: Bearer` from the same origin. A 404 is looked at a second time 1.5 s later before the page treats it as a refusal. The Worker checks the token with Kova using the Yap Admin publishable key. Sessions are app-scoped, so a Yap session does not work here.
3. The Kova user id is the Yap user id: the main app creates `users` rows keyed by it. It must be in `SITE_ADMIN_USER_IDS`, exist in `users`, and not be suspended.
4. If all of that holds, the Worker sets `__Host-yap-admin` (HttpOnly, Secure, SameSite=Strict, 12 h) and the page loads the console bundle. Every later request (page, asset or API) is checked again through a 30 s per-isolate cache. A revoked Kova session or a removed id stops working within 30 s. Only definite answers are cached: when Kova times out or answers 429/5xx, that request gets the bare 404 but the next one asks Kova again.
5. Anything else gets the same bare 404 with no body and no headers:
   - no session
   - a bad or expired token
   - another Kova app's token
   - a Kova outage
   - a signed-in non-admin; the page then signs the Kova session out
   - an unknown path, including the console's routes, its bundle and the build manifest
   - a write from another origin

Kova's hosted sign-ins return to `/?kova_auth_code=…`. A hosted sign-in started from the in-place prompt remembers the route in `sessionStorage` and opens it again afterwards.

## One-time setup

This setup needs the owner's approval, and none of it is in the repository.

1. **Kova application "Yap Admin"** (in Kova's dashboard):
   - Allowed origin: `https://<admin host>`, and nothing else.
   - Redirect / callback URI: `https://<admin host>/`, and nothing else.
   - Sign-ups: Kova has no per-app switch; the gate refuses anyone not in `SITE_ADMIN_USER_IDS`.
   - Social providers: only the ones the admins use.
   - Display name: shown on Kova's sign-in card. Use a neutral name to keep the page anonymous.
   - Copy its publishable key (`pk_live_…`).
2. **Publishable key:** `cd apps/admin && pnpm exec wrangler secret put KOVA_ADMIN_PUBLISHABLE_KEY`. The Worker writes it into the sign-in page, but it stays out of the repository because Kova ties it to the hostname.
3. **Secret:** run `cd apps/admin && pnpm exec wrangler secret put SITE_ADMIN_USER_IDS`. The value is a comma-separated list of Yap user ids.
4. **Migration:** `migrations/041_site_admin.sql` must be applied to `ralph-chat-db`. The main deploy applies it (`pnpm run migrate:main`); deploy the main Worker first.
5. **First deploy:** `pnpm admin:deploy` from the repository root runs the Vite build, then `wrangler deploy` in `apps/admin`. The main Worker (`ralph-meet`) must already be deployed with `RootMeetingRoom` and `RootVoiceRoom`, which it is.
6. **Hostname:** create a proxied DNS record for `<admin host>` (`AAAA 100::`) and a Worker route `<admin host>/*` → `yap-admin` through the Cloudflare API. Use a one-level name under the zone so the existing wildcard edge certificate covers it; a Workers Custom Domain may issue its own certificate, and every new certificate is published in Certificate Transparency logs. `workers_dev` and `preview_urls` are off, so the route is the only way in.

## Develop

From the repository root:

```sh
pnpm admin:fixtures   # the console on mock data at http://localhost:5260/?surface=admin-people&theme=dark
pnpm admin:dev        # the page without the Worker (sign-in needs VITE_KOVA_ADMIN_PUBLISHABLE_KEY)
pnpm admin:build      # dist/public plus a Wrangler dry run (dist/worker)
pnpm exec vitest run --config vitest.realtime.config.ts apps/admin/test
```

Fixture surfaces:

- `admin-people`
- `admin-person`
- `admin-live`
- `admin-insights`
- `admin-switches`
- `admin-audit`
- `admin-palette`

Add `&state=loading` or `&state=error` to show the skeletons and the error states.

## Deploying from CI (proposed, not enabled)

Nothing deploys this Worker automatically. A workflow modelled on `.github/workflows/deploy-web.yml` would be manual-only and gated by a repository variable:

```yaml
name: Deploy admin
on:
  workflow_dispatch:
    inputs:
      ref: { description: Git ref to deploy, required: false, default: main }
permissions: { contents: read }
concurrency: { group: deploy-admin, cancel-in-progress: false }
jobs:
  deploy:
    if: vars.ADMIN_DEPLOY_ENABLED == 'true'
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4.4.0
        with: { ref: "${{ inputs.ref }}", persist-credentials: false }
      - uses: ./.github/actions/setup-pnpm
      - run: pnpm exec vitest run --config vitest.realtime.config.ts apps/admin/test
      - run: pnpm admin:build
      - name: Deploy
        env:
          CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          CLOUDFLARE_ACCOUNT_ID: ${{ vars.CLOUDFLARE_ACCOUNT_ID }}
        run: cd apps/admin && pnpm exec wrangler deploy
```

The worker tests need the CI `.dev.vars` step from `ci.yml`. The deploy token needs the same Workers Scripts edit scope it has today.
