# Architecture

## Monorepo Layout
- `apps/api`: Rust backend (Axum, SQLx, Redis, R2)
- `apps/web`: React frontend (Vite, React Router, React Query)
- `docs`: operational and developer documentation

## Backend Layers (`apps/api/src`)
- `domain`: entities and repository traits
- `application`: use-case orchestration
- `infrastructure`: SQLx repositories, storage, queue, security integrations
- `presentation/http`: handlers, middleware, routes, HTTP error mapping
- `workers`: ML processing, analytics, auto-approval workers

## Backend Data Plane
- PostgreSQL (PostGIS): primary store and geospatial queries
- Redis: queue + rate-limit counters
- Cloudflare R2: image object storage
- WebSocket broadcast: processing event fan-out

## Frontend Structure (`apps/web/src`)
- `pages`: route-level screens
- `components`: reusable UI modules
- `hooks`: data/state behavior
- `lib/api.ts`: centralized API client
- `store`: Zustand client state (auth/toasts/city)

## Ownership and Auth Model
- User auth: JWT token in `ttl_user_token`
- Admin auth: JWT token in `ttl_admin_token`
- Delete ownership enforcement in backend:
  - primary: `letterings.user_id`
  - fallback for legacy rows: uploader IP match

## Request Lifecycle (Upload)
1. Frontend sends multipart upload.
2. Backend validates metadata and image.
3. Virus scan check (if enabled).
4. Image + thumbnails uploaded to R2.
5. Lettering row inserted into Postgres.
6. Optional user ownership attached (`user_id`).
7. ML job enqueued in Redis.
8. Worker updates metadata/status and emits websocket event.

## Reliability Features
- Global request IDs via `x-request-id` response header.
- Graceful shutdown in API runtime.
- CORS and security response headers.
- Automatic retry-safe client patterns in frontend data fetching.

## Quality Gates
- Frontend: lint, type-check, build
- Backend: cargo check (SQLx offline), tests
- CI workflows in `.github/workflows`
