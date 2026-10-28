# Through Your Letters

A shared archive of street lettering. Browse by city, upload photos, and save collections.

Initiated and designed by **Akankshya Pradhan**. Developed by **Vikalp Mishra**.

## Run locally

Requires Rust, Node.js 20+, pnpm 8.15.0, PostgreSQL with PostGIS, and Redis.
Cloudflare R2 credentials are needed for uploads. See [engineering](docs/ENGINEERING.md)
for configuration, failure behavior, and deployment.

```bash
pnpm install --frozen-lockfile
cp apps/api/.env.example apps/api/.env
cp apps/web/.env.example apps/web/.env
pnpm db:up
```

Fill in the environment files. Run `cargo run` from `apps/api` and `pnpm dev`
from `apps/web` in separate terminals. The API uses port 3000; the frontend uses
5173. `pnpm dev:live` in `apps/web` connects to the deployed API and changes real
records when you submit forms.

## Operational limits

Edit [runtime.json](apps/api/config/runtime.json) for the Redis budget, cache
capacity/TTL, worker waits/retries, gallery limits, and frontend cache settings.
The API validates it at startup. Set `TTL_LIMITS_FILE` to load another complete
file at runtime. Keep credentials in environment variables.

Backend overrides do not change an already-built frontend. Rebuild the frontend
after changing its `web` settings. The budget is a startup planning check, not
an account-wide billing meter.

## Check changes

```bash
pnpm --filter @ttl/web lint
pnpm --filter @ttl/web type-check
pnpm --filter @ttl/web build
cd apps/api
cargo fmt --check
SQLX_OFFLINE=true cargo check --locked --all-targets
cargo test --locked
```

Backend integration tests need PostgreSQL/PostGIS and Redis. Browser scripts in
`scripts/` require Playwright with Chromium installed in the tooling environment:

```bash
node scripts/redesign-smoke.cjs
node scripts/production-readiness-smoke.cjs
```

Run `scripts/pwa-cache-smoke.cjs` against a production build served by Vite preview
on port 5175. `TTL_PREVIEW_URL` overrides each script's default URL. UI tests use
mock API responses; they do not validate production services.

## License

[MIT](LICENSE)
