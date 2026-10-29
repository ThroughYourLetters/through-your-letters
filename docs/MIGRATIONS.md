# Database migrations and recovery

API startup validates migration history and applies pending migrations before
opening its HTTP listener or starting workers. Missing applied versions, changed
checksums, and failed migration history prevent startup. SQLx holds its PostgreSQL
advisory lock while checking/applying migrations, so concurrent API instances
serialize this work.

`IGNORE_MISSING_MIGRATIONS` defaults to `false`. Remove an existing `true` override
from Render before deploying this stricter configuration. Setting it to `true`
only permits applied versions absent from the binary; it does not permit changed
checksums or failed history. Startup logs a warning when this recovery override is
enabled. It is not an ordinary deployment or rollback setting.

## Adding migrations

Applied SQL files are immutable. Add a new forward migration with a unique version
higher than every migration on main. CI compares against the PR/merge-group/push
base and rejects edits, deletions, renames, backdated additions, and duplicate
versions. It does not modify the database history to make a check pass.

Review locks and execution time on representative data. Prefer additive changes
that remain compatible with the previous application while instances overlap.
Separately document any constraint/data conversion that prevents application
rollback. Preserve identifiers, owners, moderation state, media references, and
location provenance when backfilling records. Keep SQLx query metadata current.

## Validation

Migration tests use SQLx's separate temporary databases, including PostGIS. Set
`DATABASE_URL` to a disposable PostgreSQL server whose test role can create and
drop databases, then run this from `apps/api`:

```bash
cargo test --locked --test migrations
```

The suite exercises clean installation and repeated startup, upgrades from the
original and account-era schemas with saved uploads/comments/collections, and
missing, changed, and failed history. It checks ownership, moderation state,
coordinates, public visibility, and existing history checksums after upgrading.
Other integration tests also need `TEST_DATABASE_URL` and `TEST_REDIS_URL`.

Run `node scripts/check-migration-history.mjs BASE_COMMIT_SHA` from the repository
root to check committed changes locally. CI also runs the migration tests,
migrations, and SQLx contracts against its disposable PostGIS service.

## Before a deployment

1. Record the deployed image and applied versions with `sqlx migrate info` against
   the intended database. Check that the new image contains the complete history.
2. Take a database backup/snapshot and verify restoration into a separate database.
   Preserve the database history with the backup; SQL files alone do not restore
   application records.
3. Exercise the upgrade on that restored copy with external writes and workers
   disabled. Check the new and previous application's required queries, data
   retention, expected locks, and migration duration.
4. Deploy through the existing Render path. Require `/health` and application
   checks to pass. A migration error prevents the new API listener from starting;
   investigate it before changing traffic or retrying deployment.

## Failed deployment or rollback

- Stop the rollout and retain the failing image, logs, backup, and history. A
  checksum mismatch calls for the original SQL file from its applied release;
  do not edit `_sqlx_migrations` to accept different SQL.
- For a missing version, deploy an image containing the full applied history.
  Do not automatically set the ignore flag: it can hide that an older binary is
  incompatible with newer tables or policies.
- SQLx normally runs each PostgreSQL migration transactionally. Earlier successful
  migrations remain applied when a later migration fails. Inspect the actual
  schema/history before retrying, especially for migrations explicitly marked
  nontransactional. Reconcile partial work with a reviewed recovery procedure;
  never clear a failed marker without confirming the schema matches the SQL.
- An application-only rollback is safe only after testing that the previous
  application supports the current schema and data. Keep applied migrations in
  the rollback build. There is no automatic down-migration path in this repository.
- If the previous application is incompatible, deploy a tested forward fix or
  restore the verified backup into a separate database and validate it before
  switching over. Coordinate writes during recovery; restoring a backup can lose
  records written after that backup.
