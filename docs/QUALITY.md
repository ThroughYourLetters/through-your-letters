# Code Quality

## Build Validation

Before deploying, run:

```bash
# Backend
cd apps/api
SQLX_OFFLINE=true cargo check
cargo test

# Frontend
cd apps/web
pnpm type-check
pnpm build
pnpm lint
```

All should pass with zero errors.

## Deployment Checklist

- [ ] All environment variables set (see ENV_VARIABLES.md)
- [ ] Database migrations applied
- [ ] PostGIS extension enabled in database
- [ ] `/health` endpoint responds
- [ ] Upload → processing → approval flow works
- [ ] Admin login succeeds
- [ ] User registration and login work
- [ ] WebSocket feed connects

See DEPLOYMENT.md for full deployment instructions.
