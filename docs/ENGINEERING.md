# Engineering guide

Start with the [README](../README.md) to run the application.

## Repository layout

| Path                                              | Contents                                               |
| ------------------------------------------------- | ------------------------------------------------------ |
| `apps/web`                                        | React application, admin interface, and service worker |
| `apps/api/src/presentation/http`                  | Axum routes, request extraction, and HTTP errors       |
| `apps/api/src/application`, `apps/api/src/domain` | Application operations and domain types                |
| `apps/api/src/infrastructure`                     | PostgreSQL repositories, Redis, storage, email, and ML |
| `apps/api/src/workers`                            | Background processing                                  |
| `apps/api/migrations`, `apps/api/.sqlx`           | Database migrations and SQLx query metadata            |
| `packages`                                        | Shared frontend types, validation, utilities, and UI   |
| `scripts`                                         | Browser regression checks                              |
| `.github/workflows`                               | CI and deployment automation                           |

Trace a route and its callers before changing behavior.

## Code and review rules

- Prefer concrete functions and types. Add a trait when a real substitution
  boundary needs one. Keep a single implementation of each policy.
- Use Rust enums and validated types where they prevent invalid states. Borrow
  data when ownership need not move; clone when shared ownership requires it.
- Return errors with enough context to handle them. Never hide a failed write
  behind a default value. A panic must have a documented invariant.
- Keep database and network I/O asynchronous. Bound CPU-heavy image, password,
  and model work outside async worker threads. Bound task counts and retries.
- Explain non-obvious decisions in brief comments. Document public contracts,
  errors, and invariants with rustdoc. Omit comments that repeat code.
- Keep UI state typed. Distinguish loading, empty, and failed results; cancel
  superseded requests. Test keyboard access and small screens, including admin.
- Put operational limits in configuration. Validate units, ranges, and related
  values at startup. Add dependencies only when their maintenance cost is justified.

Keep commits focused. Explain the behavior change and its reason in the PR;
include validation results and any release steps. Update these docs when setup
or operating behavior changes. Keep investigation notes and work plans in issues
or PRs. Preserve the About narrative and project credits.

## Database and access control

- Bind query parameters. Select the fields a response needs and use typed rows;
  keep SQLx metadata current. Public responses must exclude private account and
  moderation data.
- Check ownership, role, and visibility on every affected read and write path.
  Cover anonymous, owner, unrelated user, and admin requests where applicable.
- Enforce uniqueness and related counters in the database. Use transactions for
  related writes, and test competing updates rather than relying on read-then-write.
- Bound pagination and use a deterministic sort with an ID tiebreaker. Check
  query plans on representative data before adding indexes or optimizing queries.
- Add forward migrations; never edit an applied migration. Review locks, runtime,
  compatibility with the previous release, and recovery before schema deployment.
- Do not log passwords, tokens, or raw private data. Use disposable services and
  accounts in tests. Keep production credentials out of fixtures and `VITE_*` values.

## Configuration and caching

[runtime.json](../apps/api/config/runtime.json) defines cache capacity and TTL,
queue waits and retries, page sizes, frontend cache settings, and the Redis
command budget. It contains no credentials. The API rejects unknown fields and
invalid combinations. `TTL_LIMITS_FILE` replaces the complete backend policy at
startup. The frontend reads the checked-in file at build time; rebuild it after
changing `web` settings. Keep client page sizes within the API limit.

Use [the API environment example](../apps/api/.env.example) for service settings.
Set distinct user/admin JWT secrets and explicit `ALLOWED_ORIGINS` in production.
`APP_BASE_URL` is the public frontend origin used in email links; that origin must
also route `/api` to the API. Email delivery requires `RESEND_API_KEY` and a
verified `EMAIL_FROM`. Without the key, email delivery is disabled.

| Layer                      | Behavior                                                           |
| -------------------------- | ------------------------------------------------------------------ |
| Gallery responses          | Bounded memory cache in each API process; no Redis commands        |
| API browser responses      | `Cache-Control: no-store`; no service-worker API cache             |
| React Query                | Configured freshness and retries; invalidate after relevant writes |
| Static assets              | Versioned build assets and service-worker precache                 |
| R2 images                  | Bounded service-worker cache for `*.r2.dev`; configured expiry     |
| Abuse controls and ML jobs | Redis counters and queue commands                                  |

Successful gallery writes invalidate the local response cache. A generation
check prevents an older load from refilling it after invalidation. Other replicas
can serve a cached response until its TTL expires. The memory limit counts keys
and serialized values; process overhead and active requests use additional memory.
Errors and oversized values are not cached; a zero TTL bypasses caching.

ML processing starts a consumer only when both `ENABLE_ML_PROCESSING` and
`ENABLE_ML_WORKER` are true. Enable it on the intended worker instances only;
disabling the worker does not disable job production. Producer and blocking
consumer connections are separate. Failed connections retry with bounded backoff.
The current queue uses destructive pops; a process crash can lose a claimed job.

The default Redis allowance is 50,000 commands per month. The startup check uses:

```text
idle commands = ceil(billing_days × 86400 / block_seconds) × worker_instances
idle commands + reserved_commands <= monthly_commands
```

The shipped 31-day policy estimates 8,928 idle pops for one enabled worker.
Job operations, security counters, connection setup, and other clients cost extra.
This check is a planning estimate; monitor actual provider usage. Never disable
abuse controls to fit the budget.

Offline uploads are separated by backend and build mode. The legacy `tyl-offline`
database is preserved without automatic replay because its destination is unknown.
Review pending uploads before changing a production proxy's backend. Queue
separation does not establish account ownership.

## Validation

Run frontend checks from the repository root:

```bash
pnpm --filter @ttl/web lint
pnpm --filter @ttl/web type-check
pnpm --filter @ttl/web build
```

Start disposable test services from a separate terminal:

```bash
docker run --rm --name ttl-test-db -p 5433:5432 \
  -e POSTGRES_USER=test -e POSTGRES_PASSWORD=test \
  -e POSTGRES_DB=through-your-letters-test postgis/postgis:17-3.5
```

Start Redis in another terminal with
`docker run --rm --name ttl-test-redis -p 6380:6379 redis:8.4.0-alpine`.
Wait for both services to accept connections, then run from `apps/api`:

```bash
cargo fmt --check
SQLX_OFFLINE=true cargo check --locked --all-targets
SQLX_OFFLINE=true cargo clippy --locked --all-targets -- -D warnings
TEST_DATABASE_URL=postgresql://test:test@localhost:5433/through-your-letters-test \
TEST_REDIS_URL=redis://localhost:6380 \
SQLX_OFFLINE=true cargo test --locked
```

Integration tests require a separate PostgreSQL/PostGIS database and Redis.
They apply migrations and write records. Storage and ML use test doubles.
When SQL changes, run `cargo sqlx prepare --check` against a migrated test database
with `DATABASE_URL` set and `SQLX_OFFLINE=false`; regenerate `.sqlx` as needed.
The ONNX build dependency needs its matching native runtime, even with ML disabled.

Install Chromium once, then run browser checks from the repository root:

```bash
pnpm exec playwright install chromium
node scripts/redesign-smoke.cjs
node scripts/production-readiness-smoke.cjs
node scripts/offline-queue-smoke.cjs
node scripts/pwa-cache-smoke.cjs
```

The UI scripts use mocked APIs and default to port 5173. The offline script serves
its own fixture. The PWA script needs a production build served on port 5175 with
`pnpm --filter @ttl/web preview --port 5175`. `TTL_PREVIEW_URL` overrides the UI/PWA
target. These checks do not validate real storage, email, or database behavior.

CI runs migrations, SQLx contract checks, frontend lint/types/build, and backend
check/build/tests. Formatting, Clippy, browser, and container checks must also be
run for relevant changes. Record failures and checks not run in the PR. Resolve
applicable review findings and require passing CI before merging.

## Deployment and recovery

Build from the repository root:

```bash
docker build -f apps/api/Dockerfile -t ttl-api apps/api
docker build -f apps/web/Dockerfile -t ttl-web .
```

The web container uses same-origin API calls and proxies `/api` and `/ws` to
`backend:3000`. Join both containers to a network where that hostname resolves.
For a separate API origin, pass `--build-arg VITE_API_URL=https://your-api-host`
when building the web image and allow the frontend origin in API CORS settings.
Vercel builds use the root [vercel.json](../vercel.json). Backend deployments can
use Render's GitHub integration; `RENDER_DEPLOY_HOOK` is optional.

Before release, back up PostgreSQL, review migration compatibility, set secrets,
match the configured worker count to deployment, and verify TLS and routing.
Migrations run at API startup; set `IGNORE_MISSING_MIGRATIONS=false` in production
to reject missing history. Test sign-in, upload, moderation, email links, and an
existing service worker's upgrade on designated test records. Measure idle Redis
usage after deployment; one healthy default worker issues about six pops per
30 minutes, plus setup and other activity.

`/health` checks database connectivity only. Monitor service errors and Redis
usage separately. Keep the previous application image available. Revert a frontend
release through its host; for backend rollback, first confirm the previous binary
supports the current schema. Use the migration's recovery plan when it does not.
