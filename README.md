# Through Your Letters

Through Your Letters is a platform for documenting public lettering and typography across cities.

It includes:
- A Rust API (`apps/api`) for upload, moderation, social interactions, geo features, auth, and admin tooling.
- A React web app (`apps/web`) for discovery, contribution, personal workspace, community, and moderation operations.

## Current Product Status
- Core platform is implemented and deployable.
- Major end-to-end flows exist: auth, upload, discovery, map, comments, moderation, ownership controls, and admin operations.
- Region policy controls are implemented (per-country upload/comment/discoverability toggles and moderation level).

Not fully complete yet:
- There are still roadmap items for global scale, analytics depth, experimentation, and advanced platform governance.
- "Production-ready" here means operationally deployable and maintainable, not feature-final.

## Engineering Standards Posture

What is in place:
- Typed backend and frontend contracts.
- Separation of concerns across API handlers, repositories, services, and UI layers.
- Centralized API client in web app.
- Request traceability (`x-request-id`), admin audit logging, and moderation controls.
- CI/build quality gates.

What still needs continued improvement:
- More automated tests (integration/e2e coverage expansion).
- Larger frontend bundle reduction through route-level code splitting.
- Additional observability depth and SLO-driven alert tuning.

See `docs/ROADMAP.md` for explicit backlog and priorities.

## Quick Start
```bash
pnpm install
pnpm db:up

# terminal 1
cd apps/api
cargo run

# terminal 2
cd apps/web
pnpm dev
```

Backend: `http://localhost:3000`  
Frontend: `http://localhost:5173`

## Quality Gates
```bash
pnpm lint
pnpm --filter @ttl/web type-check
pnpm --filter @ttl/web build
cd apps/api && SQLX_OFFLINE=true cargo check
```

## Security and Ownership
- User authentication with JWT.
- Admin authentication with separate token namespace.
- Upload deletion is restricted to authenticated owner accounts.
- Moderation supports automated risk scoring plus admin review controls.
- Request IDs are attached to responses (`x-request-id`) for traceability.

## Documentation
- `docs/SETUP.md` local development setup
- `docs/ENV_VARIABLES.md` required and optional environment variables
- `docs/API.md` API contract
- `docs/ARCHITECTURE.md` system architecture and module boundaries
- `docs/DEPLOYMENT.md` deployment and operational runbook
- `docs/setupoci/README.md` Oracle Cloud Always Free backend setup (Supabase + OCI runtime)
- `docs/PRODUCTION_CHECKLIST.md` release checklist
- `docs/region-policies-and-i18n.md` region policy and locale-aware search behavior
- `docs/ROADMAP.md` product roadmap and future implementation plan

## License
MIT
