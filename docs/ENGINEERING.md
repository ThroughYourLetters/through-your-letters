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

## Upload locations

Postal lookups provide approximate area coordinates, not a photo's exact position.
Failed lookups save the upload with no coordinates and a reason; these uploads
remain in the gallery but do not appear on the map. Upload responses include
`location_source` and an optional `location_warning`.

Existing coordinates are marked `legacy_unverified`. Their values are preserved
until reviewed. Map popups and upload details show the location's accuracy.

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
