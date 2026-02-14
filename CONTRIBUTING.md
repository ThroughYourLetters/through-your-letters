# Contributing Guide

Thanks for contributing to Through Your Letters.

## Development Setup

See `docs/SETUP.md` for the full local setup.

## Contribution Workflow

### 1. Clone

```bash
git clone https://github.com/akankshyasub-hash/through-your-letters.git
cd through-your-letters
pnpm install
```

### 2. Create Branch

```bash
git checkout -b feature/your-feature-name
```

### 3. Implement Changes

- Follow existing project conventions.
- Keep changes scoped and documented.
- Add or update tests for behavior changes.
- Update docs when behavior or setup changes.

### 4. Validate Locally

```bash
# Start dependencies required by backend tests
pnpm db:up

# Frontend
cd apps/web
pnpm lint
pnpm type-check
pnpm build

# Backend
cd ../api
cargo fmt --check
cargo test
```

### 5. Commit

Use conventional commits:

```text
feat: add region policy history view
fix: enforce me endpoint pagination bounds
docs: add OCI migration setup guide
test: replace upload integration smoke coverage
```

### 6. Push and Open PR

```bash
git push origin feature/your-feature-name
```

Open a Pull Request against `main`.

## Code Style

### Rust
- Run `cargo fmt`.
- Run `cargo clippy` and fix warnings for touched code.
- Keep domain behavior explicit and testable.

### TypeScript/React
- Keep types strict and explicit.
- Prefer composable components and hooks.
- Keep API interactions centralized in `apps/web/src/lib/api.ts`.

## Architecture Principles

- Domain and policy logic stays explicit.
- Infra concerns remain isolated from domain behavior.
- Route handlers stay thin and auditable.
- No silent moderation or hidden policy behavior.

## Documentation Requirements

Update docs whenever behavior changes:
- API changes -> `docs/API.md`
- Architecture changes -> `docs/ARCHITECTURE.md`
- Setup/deploy changes -> `docs/SETUP.md` and `docs/DEPLOYMENT.md`
- New platform-specific guides -> `docs/setupoci/README.md`

## Review and Merge

1. CI must pass.
2. Reviewer sign-off is required.
3. Migration changes require rollout and rollback notes.
4. Merge only after docs/tests are aligned with behavior.

## Questions

- GitHub Discussions: https://github.com/akankshyasub-hash/through-your-letters/discussions
- GitHub Issues: https://github.com/akankshyasub-hash/through-your-letters/issues
- Email: contact@throughtheletters.in

## License

By contributing, you agree that contributions are licensed under MIT.
