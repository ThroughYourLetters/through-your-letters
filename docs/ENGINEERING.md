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
| `scripts`                                         | Browser regression checks                              |
| `.github/workflows`                               | CI and deployment automation                           |

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
