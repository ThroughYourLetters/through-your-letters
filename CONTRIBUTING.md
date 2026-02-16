# Contributing

We welcome contributions. Here's how to get started.

## Quick Start

1. **Read the docs**: Start with [docs/SETUP.md](docs/SETUP.md) to set up locally.
2. **Create a branch**: `git checkout -b feature/my-change`
3. **Make changes** and test (see below).
4. **Open a pull request** with a clear description.

## Before You Code

Review relevant documentation:
- [SETUP.md](docs/SETUP.md) — how to run locally
- [ARCHITECTURE.md](docs/ARCHITECTURE.md) — project structure
- [API.md](docs/API.md) — API endpoints (if modifying backend)

## Testing & Code Style

### Rust
```bash
cd apps/api
cargo fmt        # format code
cargo clippy     # check for issues
cargo test       # run tests
```

### TypeScript/React
```bash
cd apps/web
pnpm lint        # check code style
pnpm type-check  # check types
pnpm build       # build for production
```

## Pull Request Process

1. Keep changes focused and reasonable in size.
2. Add tests for new functionality.
3. Update relevant docs (see below).
4. Use clear commit messages.
5. Ensure CI passes before requesting review.

## Documentation

Update docs when you change behavior:
- API changes → [docs/API.md](docs/API.md)
- Architecture changes → [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Setup/deploy changes → [docs/SETUP.md](docs/SETUP.md) or [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)

## Reporting Issues

Use GitHub issue templates:
- **Bug Report**: Something isn't working
- **Feature Request**: New functionality needed
- **Documentation**: Docs unclear or incorrect
- **Question**: Need help or clarification

## Code Standards

- Keep domain logic explicit (no hidden behavior).
- Isolate infrastructure concerns.
- Make handlers thin and auditable.
- No silent failures—log errors clearly.
4. Merge only after docs/tests are aligned with behavior.

## Questions

- GitHub Discussions: https://github.com/akankshyasub-hash/through-your-letters/discussions
- GitHub Issues: https://github.com/akankshyasub-hash/through-your-letters/issues
- Email: contact@throughtheletters.in

## License

By contributing, you agree that contributions are licensed under MIT.
