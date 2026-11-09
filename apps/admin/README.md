# yap-admin

The site-admin console for Yap, a separate Cloudflare Worker served only at
`https://yap-admin.115jon.com`. The main Worker (`ralph-meet`) has no admin
page or admin API.

It has five sections:

- **People:** every account with its plan, badges, stats and storage. Plans and manual badges are changed here, and sign-in can be suspended or live connections closed.
- **Live ops:** who is online, voice rooms, uploads in flight, D1 and R2 totals, the deploy, the shop catalog (last synced, items, last error, the fetcher's last push, and Refresh now) and cron runs.
- **Insights:** daily charts.
- **Switches:** kill switches and the site banner.
- **Audit log:** every admin change.

## How it is put together

| Part                                         | Where                                        |
| -------------------------------------------- | -------------------------------------------- |
| Worker: the gate, the API and assets         | `src/server/worker.ts`, `src/server/gate.ts` |
| Reads and writes (D1, the audit log)         | `src/server/admin.service.ts`                |
| Live state and disconnects (Durable Objects) | `src/server/admin-gateway.service.ts`        |
| Public sign-in page                          | `index.html`, `src/signin/main.tsx`          |
| Console page                                 | `console.html`, `src/console/*`              |
| Mock data for captures                       | `src/console/fixtures.tsx`                   |

- **Shared code.** The console imports Yap's own components, hooks, tokens and stylesheet through the `@/` alias (`../../src`); nothing is copied. Both Vite (`vite.config.ts`) and Wrangler (`tsconfig.json` paths) resolve the alias.
- **Data.** It uses the main Worker's D1, KV and R2 (the same ids as the root `wrangler.toml`). It reaches the main Worker's Durable Objects through cross-script bindings (`script_name = "ralph-meet"`), and the main Worker itself through a service binding (`YAP`, for its `/api/healthz` build stamp). It uses internal stub paths only:
  - `MeetingRoom` `/admin/introspect` and `/admin/disconnect-user`
  - `VoiceRoom` `/disconnect-participant`
- **Writes the main Worker enforces.** The main Worker reads and enforces what this Worker writes:
  - `site_settings`: the switches and the banner, through a 10 s cache.
  - `users.suspended_at`: through a 15 s cache. The disconnect closes sockets at once.

## Who gets in

Kova is the only auth, using a dedicated Kova application, **Yap Admin**:

1. `GET /` with no admin session serves a neutral sign-in page (title "Sign in"). It loads Kova's `SignIn` and nothing else. The build gives files hash-only names, and the Worker serves only the files that page needs, read from the build manifest.
2. After sign-in, the page sends its Kova app token once to `POST /api/session`, with `Authorization: Bearer` from the same origin. The Worker checks the token with Kova using the Yap Admin publishable key. Sessions are app-scoped, so a Yap session does not work here.
3. The Kova user id is the Yap user id: the main app creates `users` rows keyed by it. It must be in `SITE_ADMIN_USER_IDS`, exist in `users`, and not be suspended.
4. If all of that holds, the Worker sets `__Host-yap-admin` (HttpOnly, Secure, SameSite=Strict, 12 h). Every later request (page, asset or API) is checked again through a 30 s per-isolate cache. A revoked Kova session or a removed id stops working within 30 s.
5. Anything else gets the same bare 404 with no body and no headers:
   - no session
   - a bad or expired token
   - another Kova app's token
   - a Kova outage
   - a signed-in non-admin; the sign-in page then signs the Kova session out
   - an unknown path, including the console page and its code
   - a write from another origin

Kova's hosted sign-ins return to `/?kova_auth_code=…`, which is the sign-in page.

## One-time setup

This setup needs the owner's approval, and none of it is in the repository.

1. **Kova application "Yap Admin"** (in Kova's dashboard):
   - Allowed origin: `https://yap-admin.115jon.com`, and nothing else.
   - Redirect / callback URI: `https://yap-admin.115jon.com/`, and nothing else.
   - Sign-ups: disabled, if Kova offers the setting. Admins sign in with their existing Kova accounts.
   - Social providers: only the ones the admins use.
   - Display name: shown on Kova's sign-in card. Use a neutral name to keep the page anonymous.
   - Copy its publishable key (`pk_live_…`).
2. **Publishable key:** set `KOVA_ADMIN_PUBLISHABLE_KEY` in `apps/admin/wrangler.toml` `[vars]` to that key. It is public by design; the Worker writes it into the sign-in page.
3. **Secret:** run `cd apps/admin && pnpm exec wrangler secret put SITE_ADMIN_USER_IDS`. The value is a comma-separated list of Yap user ids.
4. **Migration:** `migrations/041_site_admin.sql` must be applied to `ralph-chat-db`. The main deploy applies it (`pnpm run migrate:main`); deploy the main Worker first.
5. **First deploy:** `pnpm admin:deploy` from the repository root runs the Vite build, then `wrangler deploy` in `apps/admin`. The main Worker (`ralph-meet`) must already be deployed with `RootMeetingRoom` and `RootVoiceRoom`, which it is.
6. **Custom domain:** in the dashboard, go to Workers & Pages, then yap-admin, then Settings, then Domains & Routes, and add `yap-admin.115jon.com`. `workers_dev` and `preview_urls` are off in `wrangler.toml`, so the custom domain is the only way in.

## Develop

From the repository root:

```sh
pnpm admin:fixtures   # the console on mock data at http://localhost:5260/console.html?surface=admin-people&theme=dark
pnpm admin:dev        # the pages without the Worker (sign-in needs VITE_KOVA_ADMIN_PUBLISHABLE_KEY)
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
