# Through Your Letters

A shared archive of street lettering. Browse by city, contribute photographs,
and save collections. Built with React, Rust/Axum, PostgreSQL/PostGIS, and Redis;
images are stored in Cloudflare R2.

## Run locally

Install rustup, Node.js 24.22.2 LTS, pnpm 8.15.0, and Docker Compose. Use
`nvm install && nvm use` to select the [.nvmrc](.nvmrc) pin, then `corepack enable`
to use the package manager declared in `package.json`. Rust commands run from
`apps/api` select Rust 1.99.1 from [rust-toolchain.toml](apps/api/rust-toolchain.toml).
The [API Dockerfile](apps/api/Dockerfile) lists native build packages. You also
need an R2 bucket and access credentials for image uploads.

From the repository root:

```bash
pnpm install --frozen-lockfile
cp apps/api/.env.example apps/api/.env
cp apps/web/.env.example apps/web/.env
docker compose up -d postgres redis
```

Edit `apps/api/.env`: replace the R2 placeholders, set distinct `JWT_SECRET` and
`ADMIN_JWT_SECRET` values, and set `ADMIN_EMAIL` and a bcrypt
`ADMIN_PASSWORD_HASH`. The example database URL matches Docker Compose.
To generate a bcrypt hash, run `htpasswd -nBC 12 admin` (from `apache2-utils` on
Debian/Ubuntu), enter a password, and copy the part after `admin:`. Keep the hash
inside single quotes in `.env` to preserve its dollar signs.
For local use without ML or automatic approval, set `ENABLE_ML_PROCESSING=false`
and `ENABLE_PENDING_AUTO_APPROVE=false`.

Start the API in one terminal:

```bash
cd apps/api
cargo run --locked
```

Start the frontend in another:

```bash
cd apps/web
pnpm dev
```

Open <http://localhost:5173>. The API runs on port 3000 and applies database
migrations at startup. Stop local services with `docker compose down`; their
data remains in Docker volumes.

`pnpm dev:live` in `apps/web` uses the deployed API on a separate frontend port
(5174). Submitting forms in that mode changes live records.

## Configuration and development

[runtime.json](apps/api/config/runtime.json) controls the Redis budget, cache
limits, worker retries, pagination, and frontend cache settings. Keep credentials
in environment variables. The frontend uses this file at build time; backend
overrides through `TTL_LIMITS_FILE` do not change built browser assets.

See [CONTRIBUTING.md](CONTRIBUTING.md) for code guidelines, database changes,
validation commands, and pull requests. The [engineering guide](docs/ENGINEERING.md)
covers the repository layout, caching, configuration, and deployment.

## Credits

Initiated and designed by **Akankshya Pradhan**.
Developed by **Vikalp Mishra**.

## License

[MIT](LICENSE)
