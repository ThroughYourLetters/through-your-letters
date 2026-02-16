# Through Your Letters

A platform for documenting public lettering and typography across cities.

## What It Does

- Upload photos of letters, signs, and typography
- Discover and explore lettering in different cities
- Comment and engage with the community
- Admin tools for moderation and content management
- Region-specific policy controls

## Tech Stack

| Part | Stack |
|------|-------|
| Backend | Rust, Axum, PostgreSQL, PostGIS |
| Frontend | React, TypeScript, Vite |
| Infra | Docker, Redis, Cloudflare R2 |

## Getting Started

### For Development

```bash
# Install dependencies
pnpm install

# Start local database and cache
pnpm db:up

# Terminal 1: Start API
cd apps/api
cargo run

# Terminal 2: Start Web app
cd apps/web
pnpm dev
```

Backend runs on `http://localhost:3000`  
Frontend runs on `http://localhost:5173`

### For Deployment

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for production setup options.

## Documentation

**Start here**: [docs/README.md](docs/README.md) — this contains a guide to all documentation.

Quick links:
- [Setup](docs/SETUP.md) — local development
- [API Reference](docs/API.md) — endpoints and contracts
- [Architecture](docs/ARCHITECTURE.md) — how the code is organized
- [Deployment](docs/DEPLOYMENT.md) — production setup
- [Diagrams](docs/DIAGRAMS.md) — system and database diagrams
- [Contributing](CONTRIBUTING.md) — how to contribute

## Code Quality

Before committing:

```bash
# Frontend
pnpm lint
pnpm --filter @ttl/web type-check
pnpm --filter @ttl/web build

# Backend
cd apps/api
cargo fmt
cargo clippy
cargo test
```

## Features

✅ User authentication and ownership  
✅ Upload, discovery, and map view  
✅ Comments and engagement  
✅ Admin moderation with audit logs  
✅ Region-specific policies  
✅ ML-powered text detection  
✅ Full-text search  
✅ Geospatial queries  

## License

MIT
