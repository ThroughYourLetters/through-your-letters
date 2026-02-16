# Deployment Runbook

This document defines a production deployment path for the current architecture.

## Target Stack
- API: Render Web Service (Docker)
- Web: Vercel Static/Vite deploy
- DB: Postgres with PostGIS extension
- Redis: managed Redis instance
- Object storage: S3-compatible storage (Cloudflare R2 or OCI Object Storage)

## Backend (Render)

### Service configuration
- Root dir: `apps/api`
- Runtime: Docker
- Health check: `/health`
- Auto deploy: enabled on `main`

### Required env vars
Use values from `apps/api/.env.example`.
Critical keys:
- `DATABASE_URL`
- `REDIS_URL`
- `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, `R2_ENDPOINT`, `R2_BUCKET_NAME`, `R2_PUBLIC_URL`
- `JWT_SECRET`
- `ADMIN_EMAIL`
- `ADMIN_PASSWORD_HASH`

### Post-deploy checks
- `GET /health` returns healthy status.
- Upload path works end-to-end.
- Admin login succeeds.
- WebSocket feed connects (`/ws/feed`).

## Frontend (Vercel)

### Build settings
- Root dir: `apps/web`
- Install command: `pnpm install`
- Build command: `pnpm build`
- Output dir: `dist`

### Required env vars
- `VITE_API_URL=https://<your-api-domain>`

### Post-deploy checks
- Browser routing works for deep links (`/about`, `/lettering/:id`, `/auth`, `/me/uploads`).
- Auth flow and uploads function against production API.

## Database and Redis
- Ensure PostGIS extension is enabled in the database.
- Run migrations before traffic cutover.
- Ensure Redis URL supports persistent connectivity from Render region.

## Keep-Alive and Uptime
If using a sleeping free tier, monitor `/health` with UptimeRobot or equivalent.

## Rollback Strategy
- Render: rollback to previous successful deploy.
- Vercel: promote previous deployment to production.
- Database: never rollback schema blindly; use forward migrations for fixes.

## Operational Checklist
- Secrets rotated and stored only in platform secret manager.
- `pnpm lint` and `pnpm --filter @ttl/web build` pass before release.
- `SQLX_OFFLINE=true cargo check` passes before release.
- Observe logs for `x-request-id` to trace incidents.

---

## Deployment

**Architecture**: Vercel (frontend) + Render (backend) + Supabase (Postgres + Auth) + Upstash/Railway (Redis) + Cloudflare R2 (storage)

**Steps**:
1. Fork the repo to GitHub.
2. Create Vercel project pointing to `apps/web`.
3. Create Render Web Service pointing to `apps/api` (Docker).
4. Create Supabase project and provision Postgres with PostGIS.
5. Provision Redis (Upstash or Railway).
6. Configure R2 bucket and obtain access keys.
7. Set all env vars in both Vercel and Render dashboards (mapped from `apps/api/.env.example` and `apps/web/.env.example`).
8. Run migrations: `SQLX_OFFLINE=true sqlx migrate run --database-url $DATABASE_URL`.
9. Test all flows: auth, upload, discovery, admin moderation.
10. Enable branch protection and CD.

### Docker Compose (Local or Self-Hosted)

**Architecture**: Single box with Docker Compose + Postgres + Redis + Nginx reverse proxy

**Steps**:
1. Clone the repo.
2. Copy `docker-compose.yml` to your server.
3. Set env vars in `.env` file at project root.
4. Run `docker-compose up -d`.
5. Run migrations: `docker exec <api_container> sqlx migrate run --database-url $DATABASE_URL`.
6. Configure DNS and HTTPS (e.g., Caddy or Let's Encrypt).

---

## Environment Configuration

See [docs/ENV_VARIABLES.md](ENV_VARIABLES.md) for the full list of required and optional variables.

**Critical vars**:
- `DATABASE_URL`: Postgres connection string with PostGIS support.
- `REDIS_URL`: Redis connection string.
- `R2_*`: Cloudflare R2 credentials.
- `JWT_SECRET`: Secret for JWT signing.
- `ADMIN_EMAIL`, `ADMIN_PASSWORD_HASH`: Admin credentials.
- `VITE_API_URL`: Frontend-to-API endpoint.

---

## Database Migrations

### Running Migrations

**Local**:
```bash
cd apps/api
sqlx migrate run --database-url "$DATABASE_URL"
```

**Via Docker** (if using docker-compose):
```bash
docker-compose exec api sqlx migrate run --database-url "$DATABASE_URL"
```

**Via Supabase** (if using Supabase project):
1. Log into Supabase dashboard.
2. Go to SQL Editor → Migrations.
3. Review and run pending migrations.

### Schema Validation

After running migrations, verify:
```bash
# Check PostGIS extension
psql $DATABASE_URL -c "SELECT * FROM pg_extension WHERE extname='postgis';"

# Check key tables exist
psql $DATABASE_URL -c "\dt" | grep -E "letterings|users|cities|comments"

# Check indexes
psql $DATABASE_URL -c "\di" | grep -E "idx_letterings|idx_fts"
```

---

## Production Readiness Checks

Before cutting traffic:

### API
```bash
# Health check
curl https://<api-domain>/health

# Admin login (test credentials in .env)
curl -X POST https://<api-domain>/api/v1/admin/login \
  -H "Content-Type: application/json" \
  -d '{"email":"...","password":"..."}'

# List letterings
curl https://<api-domain>/api/v1/letterings
```

### Frontend
- [ ] All routes load without errors.
- [ ] Auth login → Registration → Me page works.
- [ ] Upload form → preview → submit → appears in feed.
- [ ] Discovery (map, search, filter) functions.
- [ ] Admin moderation panel accessible.

### Observability
- [ ] Request IDs (`x-request-id`) appear in API responses and logs.
- [ ] Slow query logs are monitored (goal: <200ms median).
- [ ] Error rates tracked; alert threshold set (e.g., >2% errors/min).
- [ ] Database connection pool utilization monitored.

### Security
- [ ] HTTPS enforced (redirect HTTP → HTTPS).
- [ ] CORS headers whitelisted to frontend domain only.
- [ ] Admin token namespace separate from user tokens.
- [ ] Rate limiting enabled (e.g., 100 req/min per IP for public endpoints).
- [ ] Secrets never logged or exposed in error responses.
- [ ] Database backups enabled and tested.

---

## Monitoring & Alerting

### Recommended Tools
- **Logs**: Cloud provider's native logging (Render logs, Supabase logs, OCI logs).
- **Metrics**: Prometheus + Grafana (or cloud provider's native dashboards).
- **Uptime**: UptimeRobot or Datadog.
- **Error tracking**: Sentry (optional, integrates with API).

### Key Metrics to Monitor
1. **API Response Time**: p50, p95, p99 latencies (goal: <200ms, <1s, <5s).
2. **Error Rate**: errors/min, grouped by status code (goal: <1%).
3. **Request Count**: requests/min, per endpoint (detect anomalies).
4. **Database**:
   - Slow query count.
   - Connection pool utilization.
   - Replication lag (if using managed DB).
5. **Redis**:
   - Queue backlog (job.len in Queue).
   - Memory usage.
   - Eviction rate.
6. **Storage**:
   - Upload success rate.
   - Average upload duration.
   - Bandwidth usage.

### Alerting Rules
- API down (>5 consecutive health check failures) → page engineer.
- Error rate >5% → page engineer.
- Database connections >80% pool → warning.
- Disk space <10% available → critical.
- Slow query (>5s) detected → log and investigate.

---

## Rollback & Incident Response

### API Rollback (Render)
1. Render dashboard → Select web service.
2. Deployments tab → Select previous healthy deployment.
3. Click "Rollback".
4. Verify `/health` returns healthy.

### Frontend Rollback (Vercel)
1. Vercel dashboard → Go to Deployments.
2. Select previous successful deployment.
3. Click "Promote to Production".
4. Verify routes load without errors.

### Database Rollback
**Never rollback PostgreSQL schema directly.** Instead:
1. Write a new migration that undoes the breaking change (e.g., `DROP COLUMN` → `ALTER TABLE ... ADD COLUMN` with default).
2. Test migration in staging.
3. Apply to production.
4. Roll forward, never back.

### Incident Response (Checklist)
- [ ] Identify the issue via logs and error rate spikes.
- [ ] Use `x-request-id` to trace a specific request.
- [ ] Check recent deployments; rollback if necessary.
- [ ] Restart affected services (e.g., API, Redis workers).
- [ ] Clear cache if applicable (Redis FLUSHDB if safe).
- [ ] Document incident in a postmortem.

---

## Scaling Considerations

As traffic grows:

1. **Database**:
   - Enable read replicas (Supabase, managed Postgres).
   - Implement query-level connection pooling (if using PgBouncer).
   - Add indexes for frequently filtered columns (already done for FTS, location, user_id).

2. **Redis**:
   - Use Redis Cluster for high TPS.
   - Monitor queue depth; scale workers if backlog grows.

3. **API**:
   - Deploy multiple replicas behind load balancer.
   - Use container auto-scaling (Render, GKE, etc.).
   - Profile and optimize hot paths.

4. **Frontend**:
   - Enable CDN caching for static assets (Vercel does this by default).
   - Implement route-level code splitting to reduce bundle size.
   - Consider service worker for offline support.

---

## Reference Documentation

- Full API contract: [docs/API.md](API.md)
- Architecture overview: [docs/ARCHITECTURE.md](ARCHITECTURE.md)
- Region policies & i18n: [docs/region-policies-and-i18n.md](region-policies-and-i18n.md)
- Production checklist: [docs/PRODUCTION_CHECKLIST.md](PRODUCTION_CHECKLIST.md)
- Diagrams: [docs/DIAGRAMS.md](DIAGRAMS.md)
