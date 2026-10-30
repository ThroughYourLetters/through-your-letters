# Contributing

Start with the [README](README.md) for local setup. See the
[engineering guide](docs/ENGINEERING.md) for the repository layout, configuration,
and deployment.

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

## Validation

Run `pnpm check:dependencies` and `bash scripts/check-rust-dependencies.sh` when
changing dependencies. See [dependency maintenance](docs/DEPENDENCIES.md) for the
required audit tools, compatibility patches, and reviewed advisory exceptions.

Use the versions in `.nvmrc` and `apps/api/rust-toolchain.toml`. Run Rust commands
from `apps/api` so rustup selects that pin. `pnpm check:toolchains` verifies that
the developer, CI, package-engine, and container pins agree.

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
cargo run --locked --example generate_contracts -- --check
SQLX_OFFLINE=true cargo check --locked --all-targets
SQLX_OFFLINE=true cargo clippy --locked --all-targets -- -D warnings
SQLX_OFFLINE=true cargo run --locked --example onnx_smoke
DATABASE_URL=postgresql://test:test@localhost:5433/through-your-letters-test \
TEST_REDIS_URL=redis://localhost:6380 \
SQLX_OFFLINE=true cargo test --locked
```

Each integration test creates an isolated PostgreSQL/PostGIS database using
`DATABASE_URL`; the test role must be able to create and drop databases.
Use a disposable Redis instance for `TEST_REDIS_URL`. Storage, ML, and external
providers use test doubles or local servers. See the
[migration guide](docs/MIGRATIONS.md) for upgrade and recovery checks.
When SQL changes, run `cargo sqlx prepare --check` against a migrated test database
with `DATABASE_URL` set and `SQLX_OFFLINE=false`; regenerate `.sqlx` as needed.
Cargo does not download native ONNX binaries. The enabled detector and
`cargo run --locked --example onnx_smoke` need Runtime 1.23.2 selected through
`ORT_DYLIB_PATH`; see the README's checksum-verified installer instructions.

Install Chromium once, then run browser checks from the repository root:

```bash
pnpm exec playwright install chromium
node scripts/redesign-smoke.cjs
node scripts/production-readiness-smoke.cjs
node scripts/map-smoke.cjs
node scripts/city-search-smoke.cjs
node scripts/location-repair-smoke.cjs
node scripts/metadata-edit-smoke.cjs
node scripts/offline-queue-smoke.cjs
node scripts/account-boundary-smoke.cjs
node scripts/postal-code-smoke.cjs
pnpm --filter @ttl/web build
node scripts/pwa-cache-smoke.cjs
```

The UI scripts use mocked APIs and default to port 5173. The offline script serves
its own fixture. Account and postal-code checks start their own Vite servers.
The PWA script starts a preview server for the production build; `TTL_PREVIEW_URL`
can select an existing preview. These checks do not validate real storage, email,
or database behavior.

CI runs migrations, SQLx query checks, API type generation, formatting, Clippy,
frontend lint/types/build, backend tests, browser privacy/upload checks, and
container smoke tests. Resolve applicable review findings and require passing CI
before merging.

For container changes, build both images and run `node scripts/container-smoke.mjs`
as described in the engineering guide. It creates and removes its own test
PostGIS/Redis services, checks native inference and migrations, and exercises
nginx routing and WebSockets. The Containers workflow runs the same check.
