# Redesign integration

The integration branch merges `akankshya22/through-your-letters--redesign` at
`3a70f054e8dd915832b266355900278275825404` into the main repository, preserving
its commits and authorship. Its shared ancestor is the original production
commit `b409d0faa491b463506c2d7eb4607d7328be9166`.

The change brings the editorial gallery, responsive navigation, typography,
map and form styling, and interactive hero into the existing React app.
Backend code, migrations, shared packages, deployment workflows, and production
hosting configuration are unchanged. No data migration is required.

## Integration fixes

- Refresh gallery queries after deleting an upload from the gallery or detail page.
- Avoid fetching another page of placeholder results during filter transitions.
- Clear previous comments when changing entries and surface comment-loading failures.
- Give the compact home link and mobile comment submit button accessible names.
- Return focus to the city picker trigger after selection or Escape; use dialog
  semantics for the search field and ordinary keyboard-accessible city buttons.
- Honor reduced motion for CSS animations, transitions, and scripted scrolling.
- Keep the contribution heading and cancel button inside 320px screens.

`dev:live` uses the public API address in `apps/web/.env.live`. This file has no
credentials. Its mode is only loaded by `vite --mode live`; normal production
builds continue to use the hosting provider's `VITE_API_URL` configuration.
The removed `ZinePage.tsx` is replaced by the new gallery cards; no component
imports remain. The existing `ZinePageData` interface is still used by those cards.

## Validation

Use the repository's pnpm version (8.15.0) to preserve its existing lockfile:

```bash
npx --yes pnpm@8.15.0 install --frozen-lockfile
npx --yes pnpm@8.15.0 --filter @ttl/web lint
npx --yes pnpm@8.15.0 --filter @ttl/web type-check
npx --yes pnpm@8.15.0 --filter @ttl/web build
```

The browser smoke check uses Playwright from the tooling environment. If it is
not already installed, install it outside the workspace:

```bash
npm install --prefix /tmp/ttl-browser-tools playwright@1.62.1
NODE_PATH=/tmp/ttl-browser-tools/node_modules node /tmp/ttl-browser-tools/node_modules/playwright/cli.js install chromium
```

Start the local frontend in one terminal:

```bash
cd apps/web
VITE_API_URL=same-origin ./node_modules/.bin/vite --host 0.0.0.0
```

Run the check from the repository root in another terminal:

```bash
NODE_PATH=/tmp/ttl-browser-tools/node_modules node scripts/redesign-smoke.cjs
```

If Playwright is already provided by the environment, omit `NODE_PATH`.
`TTL_PREVIEW_URL` overrides the default `http://localhost:5173`.
`TTL_ARTIFACT_DIR` overrides the default `/tmp/ttl-redesign-artifacts`.
The runner mocks API calls and the WebSocket feed, blocks service workers, and
checks desktop (1440px), mobile (390px), and narrow mobile (320px). Screenshots
use synthetic image fixtures. These checks do not establish production account,
email, storage, moderation, or offline-upload behavior.

## Review and release

Review the pull request and its preview before merging. After merging, verify
the production gallery, navigation, detail links, login, and map. An authorized
maintainer can verify uploads and moderation with designated test records.
If rollback is needed, restore the previous frontend deployment through the
hosting dashboard. The previous production Git commit is recorded above; no
backend or database rollback is needed for this frontend integration.
