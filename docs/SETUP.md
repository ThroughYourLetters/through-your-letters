# Local Setup

## Prerequisites
- Node.js 20+
- pnpm 8+
- Rust stable
- Docker (for local Postgres/Redis)

## 1) Install Dependencies
```bash
pnpm install
```

## 2) Configure Environment
```bash
cp apps/api/.env.example apps/api/.env
cp apps/web/.env.example apps/web/.env
```

Edit `apps/api/.env` with valid R2 and auth values.

## 3) Start Local Infrastructure
```bash
pnpm db:up
```

## 4) Run Backend
```bash
cd apps/api
cargo run
```

Backend available at `http://localhost:3000`.

## 5) Run Frontend
```bash
cd apps/web
pnpm dev
```

Frontend available at `http://localhost:5173`.

To work on the frontend without running the backend locally, use the deployed API instead:

```bash
cd apps/web
pnpm dev:live
```

The dev server proxies `/api` and `/ws` to the backend named in `apps/web/.env.live`. Likes, comments and uploads made this way go to the live archive.

## 6) Validate End-to-End
- `GET http://localhost:3000/health` returns healthy JSON.
- Frontend loads gallery.
- Upload succeeds and appears in gallery.
- Auth flow works: register -> login -> `My Uploads`.

## Useful Commands
```bash
pnpm lint
pnpm --filter @ttl/web type-check
pnpm --filter @ttl/web build
cd apps/api && SQLX_OFFLINE=true cargo check
```

## Shutdown
```bash
pnpm db:down
```
