# Engineering guide

Start with the [README](../README.md) for local setup. See
[CONTRIBUTING.md](../CONTRIBUTING.md) for code guidelines and validation commands.

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
| `scripts`                                         | Toolchain checks and browser regression checks         |
| `.github/workflows`                               | CI and deployment automation                           |

## Toolchains and locked builds

Rust commands run from `apps/api` use its `rust-toolchain.toml` pin, including
rustfmt and Clippy. `Cargo.toml` declares the same tested compiler as `rust-version`.
Keep this file inside the API build context so the existing Render Docker setup
continues to work. Running `cargo --manifest-path apps/api/Cargo.toml` from the
repository root does not select that directory's rustup override; change directory
first or explicitly select the pinned toolchain.

`.nvmrc` selects an exact Node 24 LTS release for development and CI. Docker uses
the same release. Vercel manages its own patch rollout, so `engines.node` selects
`24.x` rather than requiring that host to provide the newest upstream patch.
The pnpm engine range accepts compatible version 11 releases; development, CI,
Vercel, and Docker use the exact `packageManager` pin through Corepack.
See [Vercel's Node version contract](https://vercel.com/docs/functions/runtimes/node-js/node-js-versions).
CI enables Corepack after selecting Node, and Vercel invokes Corepack explicitly
instead of inferring a pnpm version from the lockfile. Use frozen pnpm installs
and `--locked` Cargo commands. The SQLx CLI is fixed to 0.8.6 with its
published dependency lockfile, matching the application's SQLx 0.8 metadata.

Run `pnpm check:toolchains` after changing a version. CI uses this check to select
Rust and rejects mismatches between toolchain files, package engines, and Docker
builder tags. Update each matching pin together. Major pnpm and application
dependency upgrades are reviewed separately from compiler/runtime upgrades.

ONNX compatibility also depends on its native runtime. The current `ort`
2.0.0-rc.11 binding uses ONNX Runtime 1.23.2. Compiler validation must include the
matching library and `cargo run --locked --example onnx_smoke`, which runs an
inference with the shipped model and a generated image. It checks compatibility,
not recognition accuracy. Disabling ML alone does not verify native compatibility.
Cargo uses dynamic loading and never downloads the native runtime. The Linux
installer at `apps/api/scripts/install-onnx-runtime.sh` verifies official x86_64
and aarch64 CPU archives against pinned SHA-256 digests. CI and the API Dockerfile
use this same installer. Local enabled-ML runs need `ORT_DYLIB_PATH` set to the
installed library; the API image sets it to its bundled 1.23.2 library.

## Accounts and privacy

Password resets and account deletion revoke existing user sessions. Protected
requests check the account's current session version and active state.

Deleting an account removes its identity, recovery tokens, notifications, and
private collections. Public uploads and public collections remain under
`Anonymous`; comments retain their place in the thread with `[deleted]` text.
Uploader IPs and internal report reasons are excluded from public upload JSON.

Public discovery, related uploads, and counts include only approved uploads from
discoverable regions. The `discoverable_letterings` view defines this policy.
`readable_letterings(requester)` also allows owners to preview their own hidden
uploads. Every upload in a revisit must be readable by the caller. Collection
items and their counts use public discovery visibility, including owner views.
The anonymous live feed sends events only for discoverable uploads.

Administrators review hidden uploads through the protected admin routes. An admin
token does not grant access to user routes. These rules govern API disclosure;
previously obtained public R2/CDN image URLs are not revoked by hiding an upload.

## Client addresses and abuse controls

Render provides the production API's reverse proxy. Its ingress passes through
Cloudflare and Render load balancers, so the TCP peer is normally a proxy rather
than the visitor. See [Render's client-IP guidance](https://render.com/articles/how-render-handles-ddos-attacks).

The existing deployment needs no additional reverse proxy or proxy allowlist.
When Render sets `RENDER=true` and `RENDER_SERVICE_TYPE=web`, the API automatically
uses `CF-Connecting-IP`, which Cloudflare replaces with the address connecting to
its edge. See [Render's platform variables](https://render.com/docs/environment-variables)
and [Cloudflare's header contract](https://developers.cloudflare.com/fundamentals/reference/http-headers/#cf-connecting-ip).
Caller-supplied `X-Forwarded-For`, `X-Real-IP`, `True-Client-IP`, and `Forwarded`
never override that address. Missing edge headers (for example, health probes)
use the TCP peer; duplicate or malformed edge addresses return 400. There is no
fallback to a caller-controlled forwarding chain.

This mode trusts Render's public ingress and services in the same workspace's
private network. Internal callers must not copy an untrusted `CF-Connecting-IP`
header into a request to the API. An additional upstream CDN or frontend rewrite
is identified by its connection to Render; do not use its unverified forwarding
headers to recover an earlier visitor. Outside Render, vendor headers are ignored.

For other deployments, `TRUSTED_PROXY_CIDRS` accepts verified ingress proxy networks
and defaults to no trusted networks. Only a trusted TCP peer can supply `X-Forwarded-For`;
the API walks that chain from the right and stops at the first untrusted address.
It validates every address and bounds the chain with `security.max_forwarded_hops`.
Malformed trusted headers return 400. IPv4-mapped addresses use canonical IPv4
identities. Do not trust all Internet addresses or use Render's outbound IP list
as an ingress allowlist. The same resolved address is used for limits, likes, and
stored uploader/comment IPs in either mode.

The nginx container and Vite development proxy are alternative edge entrypoints.
They replace caller-supplied forwarded addresses with their socket peer address.
Trust their actual backend-facing peer addresses only when using those paths.
Adding another proxy in front requires reviewing that proxy's trust boundary too.
Email links always use the configured `APP_BASE_URL`, never request Host headers.

Protected actions return 429 when their quota is exhausted and 503 when Redis
cannot enforce it. Registration, login, recovery, password reset, admin login,
uploads, comments, and reports never proceed after a limiter failure. Public reads
remain available without Redis. Zero upload limits are rejected at startup.

The `security` runtime policy defines request counts, window lengths, and bounded
Redis connection/response timeouts. Include it in complete runtime overrides.
Windows start with the first counted attempt; rejected attempts do not extend
them. Comments and reports are limited by account and upload, so changing IPs
does not reset those quotas. These rules use one shared reconnecting connection,
atomic Lua operations, and no idle polling. Provider billing includes connection
setup and may account for script operations; monitor actual command usage.

## Upload locations

Postal lookups provide approximate area coordinates, not a photo's exact position.
Failed lookups save the upload with no coordinates and a reason; these uploads
remain in the gallery but do not appear on the map. Upload responses include
`location_source` and an optional `location_warning`.

Changing an upload's postal code clears its coordinates and marks its location
for review. Other metadata edits preserve the location. Postal edits and reviewed
corrections lock the same upload, so they cannot leave stale coordinates verified.
Existing correction evidence and metadata history remain available.

Existing coordinates are marked `legacy_unverified`. Their values are preserved
until reviewed. Map popups and upload details show the location's accuracy.

### Map density

The map draws a continuous heat layer over the tiles. Colours show relative upload
density in the current view. Postal coordinates remain approximate. Unverified,
unresolved, and polar locations outside Web Mercator are counted separately.

The map and gallery share approval, region, script, and style filters. Viewport
requests aggregate uploads into stable Web Mercator cells. Cells grow when needed
to stay within `map.max_cells`; responses indicate any truncation. Date-line views
use two longitude ranges. Hotspot photos use bounded pages and an ID cursor.

The `map` section in [runtime.json](../apps/api/config/runtime.json) controls cell
size, query timeout, request debounce, query retention, heat radius, blur, and the
zoom level for individual pins. Include it in custom runtime policies. Rebuild the
web app after changing these settings. `heat_saturation_count` sets a lower bound
for the colour scale, so a lone upload does not appear as a busy hotspot.

Density and location counts use the bounded public memory cache, with no Redis
commands. Each map query has a database statement timeout. Upload and moderation
writes invalidate the local cache; other replicas can serve data until expiry.

### Repairing existing coordinates

Open **Admin > Locations** to review unresolved and unverified uploads. Read the
linked geographic source and enter its city, country code, postal code, and
coordinates. Use a postal authority or a map source you have checked.

Preview shows the current and proposed coordinates without changing the upload.
Confirm the evidence before approving. The API checks coordinate bounds and that
the source's city, country, and postal code match the upload. The reviewer checks
the coordinates against the source; matching names alone do not verify geography.
Mismatched city or postal details must be resolved before a coordinate repair.

Approvals save coordinates, source evidence, reviewer, and previous values in one
transaction. Changed uploads require a fresh preview. Retrying the same approval
returns its saved result. Corrected uploads leave the report; refresh it to resume
reviewing the remaining records. Enable **Show resolved uploads** to revisit earlier corrections. History remains
available through the admin history endpoint after upload deletion.
Postal corrections remain approximate and do not become GPS positions.

`location_repairs.max_evidence_bytes` limits the combined source text per correction.
Include this section in complete runtime configuration overrides.

## Configuration and caching

[runtime.json](../apps/api/config/runtime.json) defines cache capacity and TTL,
queue waits and retries, page sizes, frontend cache settings, and the Redis
command budget. It contains no credentials. The API rejects unknown fields and
invalid combinations. `TTL_LIMITS_FILE` replaces the complete backend policy at
startup. The frontend reads the checked-in file at build time; rebuild it after
changing `web` settings. Keep client page sizes within the API limit.
`web.geolocation_timeout_seconds` bounds device-location requests from 1 to 60
seconds. Include this field in complete configuration overrides.

Use [the API environment example](../apps/api/.env.example) for service settings.
Set distinct user/admin JWT secrets and explicit `ALLOWED_ORIGINS` in production.
`APP_BASE_URL` is the public frontend origin used in email links; that origin must
also route `/api` to the API. Email delivery requires `RESEND_API_KEY` and a
verified `EMAIL_FROM`. `APP_BASE_URL` must be an HTTPS origin, with no path, query,
fragment, or credentials; HTTP is allowed for localhost development. A trailing
slash is supported. Invalid link configuration prevents API startup.

Without a nonempty key, email delivery is disabled and startup emits a warning.
Send attempts return an explicit disabled-delivery error, which account handlers
log without tokens or email addresses. Provider errors report only a safe category
and HTTP status; response bodies are never logged because they can echo links.
Registration still creates the account when email delivery fails. Password-reset
requests keep the same neutral response for existing and unknown accounts.

`email.request_timeout_ms` bounds provider requests (100–30000 milliseconds;
default 10000). Requests reuse one client, follow no redirects, and are not retried
automatically. Include the `email` section in complete runtime policy overrides.

| Layer                      | Behavior                                                           |
| -------------------------- | ------------------------------------------------------------------ |
| Geocoding searches         | Bounded PostgreSQL cache and shared provider rate gate             |
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

Typing in the city picker searches saved cities only. Search worldwide starts
an explicit provider lookup. [Nominatim’s policy](https://operations.osmfoundation.org/policies/nominatim/)
does not permit autocomplete requests.
Postal lookups and city discovery share one Nominatim client per API process.
The `geocoding` policy sets its endpoint, timeouts, response size, cache capacity,
and request spacing. All replicas must share PostgreSQL and the same policy.
Calls from other applications are outside this gate. Coordinate their provider
budget separately.

Successful results stay cached for 30 days; empty results stay for one hour.
Expired rows are removed on writes; the oldest rows are evicted at capacity.
The defaults allow 10,000 entries of at most 256 KiB each. Database storage and
indexes need additional space. These lookups use no Redis commands.

Cache hits work during provider cooldowns. An uncached lookup claims one shared
lease; competing lookups fail promptly instead of queuing. Requests are at least
1.1 seconds apart, with no automatic retries or redirects. Provider failures
start a 60-second cooldown; `Retry-After` can extend it. A cancelled request holds
its lease until the total timeout plus request interval passes. Cache or database
failures prevent external requests.

Uploads survive failed lookups without map coordinates. Admin > Locations can
review them later; no background geocoding retry runs. Include `geocoding` in
complete runtime overrides before deploying. The migration adds only cache and
gate tables; an application rollback can leave them in place. Check provider
terms and OpenStreetMap attribution when changing the endpoint.

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

## Deployment and recovery

Build from the repository root:

```bash
docker build -f apps/api/Dockerfile -t ttl-api:test apps/api
docker build -f apps/web/Dockerfile -t ttl-web:test .
node scripts/container-smoke.mjs
```

The web container uses same-origin API calls and proxies `/api` and `/ws` to
`backend:3000`. Join both containers to a network where that hostname resolves.
For a separate API origin, pass `--build-arg VITE_API_URL=https://your-api-host`
when building the web image and allow the frontend origin in API CORS settings.
Vercel builds use the root [vercel.json](../vercel.json). Backend deployments can
use Render's GitHub integration; `RENDER_DEPLOY_HOOK` is optional.

The Containers workflow builds both images and runs the smoke script against
disposable PostGIS/Redis services. It verifies native library resolution and real
model inference, database migrations at startup and restart, health, SPA routes
and assets, proxied JSON endpoints, and WebSocket upgrades. The script removes
only its own containers/network. `TTL_API_IMAGE` and `TTL_WEB_IMAGE` override the
image tags. Render continues to use its existing API Docker build context.

Before release, back up PostgreSQL, review migration compatibility, set secrets,
match the configured worker count to deployment, and verify TLS and routing.
Migrations run before the API listener and workers start. Missing history is
rejected by default; remove existing `IGNORE_MISSING_MIGRATIONS=true` overrides.
Follow the [migration and recovery guide](MIGRATIONS.md) for immutable SQL,
upgrade checks, and compatible rollback/restore steps.
Test sign-in, upload, moderation, email links, and an
existing service worker's upgrade on designated test records. Measure idle Redis
usage after deployment; one healthy default worker issues about six pops per
30 minutes, plus setup and other activity.

`/health` checks database connectivity only. Monitor service errors and Redis
usage separately. Keep the previous application image available. Revert a frontend
release through its host; for backend rollback, first confirm the previous binary
supports the current schema. Use the migration's recovery plan when it does not.
