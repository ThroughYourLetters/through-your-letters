# Engineering

## Caching and Redis command budget

This policy targets the reported **50,000 Redis commands per month**. It does
not assume that page views equal Redis commands. Security counters and job
operations still use Redis; public browsing no longer does.

## Why low traffic could exhaust the allowance

The previous ML consumer started unconditionally, including when ML processing
was disabled. An empty queue caused BRPOP every five seconds on a newly opened
connection. One always-on consumer issued about **518,400 empty-queue commands
per 30 days**, before AUTH/connection setup. Failures retried every second.
The gallery cache also waited for a distributed lock by sending up to 60 GETs
per waiting request, and used extra writes for lock/stale copies.

There was also a frontend search loop: response callbacks changed on every
parent render, which retriggered the effect and the same search. Search itself
queries PostgreSQL, so this amplifies HTTP/database traffic rather than being
a direct Redis command source. It is fixed independently.

These are code-level causes, not a measurement of the production account. In
Upstash Usage, inspect Top Commands Usage to distinguish BRPOP, AUTH/CLIENT,
GET/SET, rate-limit scripts, and other clients. The Upstash console's Data Browser
also issues commands; don't leave it refreshing while measuring idle usage.
See [Upstash's command-count guidance](https://upstash.com/docs/redis/troubleshooting/command_count_increases_unexpectedly).

## Layer policy

| Layer | Policy | Limits / invalidation | Redis cost |
| --- | --- | --- | --- |
| Static build assets | Hashed files + service-worker precache | New deployment replaces shell | None |
| Public image assets | Existing browser/CDN cache + CacheFirst service worker | 200 entries, 7 days in service worker | None |
| Gallery API | Per-process in-memory response cache | 15 seconds; 256 entries; 16 MiB serialized keys/values; 512 KiB maximum value | None |
| Other API responses | Network only, `Cache-Control: no-store` | No persisted account/admin/API responses | None for cache |
| React Query | Existing five-minute freshness for the gallery UI | Explicit invalidation on contribution/delete; no background polling | None directly |
| ML queue | Dedicated, reused Redis TCP connection; BRPOP waits 300 seconds | Job arrival wakes the blocked request immediately | About 8,640 idle pops / 30 days / enabled consumer |
| Failed queue connections | Backoff: 5, 10, 20, 40, 80, 160, 320, 640, then 900 seconds | Reuse producer connection; no once-per-second failure loop | Variable; monitor failures |
| Abuse prevention | Existing Redis-backed limits remain enforced | Never disabled to save cache quota | Per actual protected operation |

The memory bound counts serialized payload and key bytes. HashMap/allocation
metadata and active requests add overhead; 16 MiB is not a total process-memory
limit. Excessively large responses are served without caching. One of the configured
load locks coalesces same-key misses without Redis polling or unbounded lock maps.
Errors are not cached. Structured, normalized filter keys avoid accidental
collisions and duplicate entries for equivalent requests.

Successful gallery-affecting HTTP writes invalidate this instance's cache.
ML and auto-approval workers invalidate their local cache after updating rows.
A generation check prevents an older in-flight load from repopulating entries
after invalidation. No stale Redis fallback remains.

With several API replicas, local invalidation is immediate only on the writing
replica; a different replica may serve up to 15 seconds of cached gallery data.
There is no claim of cross-replica strong consistency. Use one replica at this
traffic level, or add event-driven cross-replica invalidation if moderation must
be visible everywhere immediately. Client gallery UI state is separate: an
already-open page may show its previous results until navigation/refetch or an
invalidation. This policy does not make image URLs private or erase CDN copies.

The new service worker deletes legacy `api-cache` during activation. API calls
never use it again. API fetches also request `cache: no-store`. Deploy the built
`cache-policy.js` with the service worker; verify migration on an existing client.

## Worker configuration and monthly budget

- `ENABLE_ML_PROCESSING=false`: no queue consumer, zero idle queue commands.
- `ENABLE_ML_PROCESSING=true` and `ENABLE_ML_WORKER=true` (default): run a consumer.
- Set `ENABLE_ML_WORKER=false` on extra API replicas. Keep one enabled instance
  if uploads enqueue ML jobs. This flag does not disable job production.
- BRPOP uses the Redis TCP/TLS URL, not the REST API. A producer connection is
  separate so a blocked consumer cannot delay enqueueing. The client response
  timeout is 305 seconds, longer than the command's 300-second wait. See
  [Upstash BRPOP semantics](https://upstash.com/docs/redis/commands/list/brpop).

For a 30-day month:

```text
idle queue commands = 8,640 × enabled consumers
remaining allowance = 50,000 - idle queue commands
actual total = idle + job operations + security counters + reconnect/setup + other clients
```

One enabled consumer leaves approximately **41,360 commands** for useful work
and overhead. Six idle consumers alone would exceed the allowance. A 31-day
month has 8,928 idle pops per consumer. Error replies, connection limits, and
provider/network interruptions can change measured totals, so this is a planning
estimate, not a billing guarantee.

A reasonable initial allocation is 10k for one idle consumer, 30k for protected
operations/jobs, and 10k for overhead/headroom. Set usage alerts at 25k, 37.5k,
and 45k in the provider dashboard; this code does not configure those alerts.
If low-volume traffic still exceeds the budget, identify the top command first.
Do not remove login/upload/comment limits. The next architectural step would be
an event-driven/durable job queue with no idle Redis polling, rather than a
longer cache TTL or silently skipping abuse controls.

## Checks and rollout

- `scripts/production-readiness-smoke.cjs`: fixed navigation through scrolling
  and transformed parent containers, footer clearance, one request per search,
  cancelled/superseded searches, 320/390/640/1024/1440px.
- `scripts/pwa-cache-smoke.cjs`: run on a production build served by Vite preview;
  removes a seeded legacy private cache, verifies no new API cache entries, and
  verifies no private API response is returned while offline.
- Cache unit tests: coalescing 100 simultaneous readers, expiry, invalidation,
  invalidation during an in-flight load, failed loads, capacity and TTL bounds.
- Queue tests use a local RESP fixture, count reused/separate connections,
  verify the 300-second wire argument, and wake the consumer after enqueueing.
- HTTP middleware tests cover successful/failed-write invalidation and no-store
  on successful/missing API responses without changing `/health`.

Before deployment, review the diff and confirm the worker count. After frontend
and backend deployment, measure Redis commands during a 30-minute idle interval
without browsing the console keys. One healthy enabled consumer should contribute
about six idle pops in that interval, plus initial setup/other activity. Browse
and search a few times: there should be no gallery-cache GET/SET/lock commands.
Exercise an upload and moderation change using designated test records, then
check cache refresh and service-worker upgrade behavior.

These changes address the layout and caching/request-volume issues. The remaining engineering work below must be resolved before full production sign-off.


## Runtime configuration

`apps/api/config/runtime.json` is the checked-in operational policy. It contains
no credentials. The backend bundles it; `TTL_LIMITS_FILE` can replace it with a
complete JSON file without rebuilding the API. Unknown fields, zero waits,
invalid capacities, incompatible page sizes, overflow, and an over-budget idle
worker estimate cause startup to fail. Configure the expected worker count to
match deployment; the API cannot discover other replicas or external clients.

`redis_budget` holds the allowance, reserve, billing period, and worker count.
`cache` controls TTL, capacity, value size, and coalescing lanes. `queue` controls
blocking waits, connection/response timeouts, retry bounds, job attempts, and
HTTP timeout. `gallery` controls API page limits. `web` controls gallery page
size, React Query freshness/retries, and the service-worker image cache.

`uploads_per_ip_per_day` supplies the upload-limit default; an explicit
RATE_LIMIT_UPLOADS_PER_IP environment value takes precedence.

All numbers in the policy table describe the shipped config, not fixed
algorithm requirements. Zero cache TTL bypasses caching. Each key's requested
TTL is capped by the configured cache TTL. The web uses the same checked-in
file at build time; a backend runtime override cannot reconfigure browser assets.
Rebuild/redeploy both when changing shared page-size assumptions. The client
also respects the API's returned page limit.

## Services and secrets

Use `apps/api/.env.example` and `apps/web/.env.example` as the full environment
reference. Required backend settings are DATABASE_URL, REDIS_URL, R2 access key,
secret, endpoint, bucket and public URL, JWT_SECRET, ADMIN_EMAIL, and a bcrypt
ADMIN_PASSWORD_HASH. Set a distinct ADMIN_JWT_SECRET in production. Never put
credentials in JSON policy or VITE_* variables.

Use ALLOWED_ORIGINS for frontend origins, VITE_API_URL for the public backend,
and APP_BASE_URL for the frontend links in email. RESEND_API_KEY and EMAIL_FROM
configure email delivery. Verify email links through the actual deployment;
frontend and API origins need correct routing. CORS allows PATCH for upload edits.

The backend is Axum + SQLx/PostGIS. HTTP handlers call repositories/storage and
workers. Redis remains involved in security counters and ML jobs. The public
script value Latin is displayed as English / Latin; no language is inferred.

## Deployment and recovery

Build the backend with `apps/api/Dockerfile` using `apps/api` as its context;
its config directory must be copied into the build. The ONNX dependency needs
a working binary download or an installed matching runtime via ORT_LIB_LOCATION.
Local validation used Microsoft's ONNX Runtime 1.23.2 release after the default
CDN returned 403. The backend container still needs independent validation.

Build the frontend from the repository root with
`docker build -f apps/web/Dockerfile .`; it needs the shared config and workspace
packages. The image uses same-origin API calls by default, and nginx proxies
API and WebSocket traffic to the `backend:3000` service. Override VITE_API_URL
with a Docker build argument for a separate API origin. Use the checked-in
Vercel root config for hosted frontend builds. Confirm production
VITE_API_URL, SPA routing, allowed origins, TLS, and email routing in hosting
settings. Render can deploy through its GitHub integration; RENDER_DEPLOY_HOOK
is optional. GitHub CI uses isolated test credentials and database services;
never reuse production storage or accounts for tests.

Migrations run at API startup. Back up PostgreSQL before schema releases and
check migration drift. Do not rewrite applied migrations. Revert a frontend
release using the previous hosting deployment. For backend/schema failures,
restore the compatible application and follow the migration's recovery plan.
`/health` checks database connectivity, not R2, Redis, or model correctness.

## Remaining engineering work

The current PR includes the redesign, layout/copy corrections, typed admin
activity, request cancellation, configurable local caching, and queue savings.
It does not complete the broader refactor. Remaining priorities:

- Public DTO privacy and consistent moderation/owner checks on every read path.
- Session revocation, atomic resets, deletion cleanup, and safe email failures.
- Concurrent like/counter updates, atomic throttling, query plans and indexes.
- Bounded blocking execution for image/password/model work and durable jobs.
- Removal of unused parallel validation/use-case/monitoring code after tracing callers.
- Typed admin feature boundaries, bulk-action tests, and real service observability.
- Aligned Rust toolchains, strict Clippy, dependency checks and container tests.

Comments should explain a non-obvious reason briefly. Public contracts should
name invariants and errors. Avoid cosmetic abstraction changes without a clear
maintenance benefit. About's personal narrative stays intact.
