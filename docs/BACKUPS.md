# Database backups

The **Database Backups** workflow exports the application's PostgreSQL `public`
schema and SQLx migration history every day at 02:43 UTC. It restores the dump
into a new, isolated PostGIS database before encrypting it with age. GitHub keeps
only the encrypted archive and a receipt with its checksum and creation time.
The decryption key stays outside GitHub.

This covers application accounts, uploads' database records, moderation, and
other application tables. It does not back up Supabase's managed Auth/Storage
schemas, database roles, or the image bytes stored in R2. Image and CDN retention
remain separate, as described in [the operator guide](ENGINEERING.md).

## Activate once

1. Save the supplied `.agekey` recovery file in your password manager or another
   durable private location. Its public recipient is in
   [the backup policy](../ops/database-backup.json). The private key must not be
   committed or added to GitHub Actions. It is needed to recover any export.
2. Add the repository Actions secret **`DATABASE_BACKUP_URL`**, using the value
   of Render's existing `DATABASE_URL`. For Supabase, use the IPv4 session pooler
   on port **5432**, not the transaction pooler on 6543. The workflow requires
   TLS for remote connections and adds `sslmode=require` when unspecified.
3. Run **Database Backups** on **main** once. Confirm that the `backup` job succeeds
   and has a `database-backup-<run>-<attempt>` artifact. Download it and perform the
   recovery drill below before treating production recovery as verified.

No Supabase plan change or new paid storage is required. Production backups are
not active until the secret is set and the first run succeeds. Missing credentials
fail the job; they do not produce a successful empty backup.

## Retention and alerts

[The policy](../ops/database-backup.json) keeps at most seven exports for up to
seven days, with a **25 MiB** limit per encrypted archive. Older backup artifacts
are deleted only after the new encrypted artifact is uploaded and confirmed to
exist. Other CI artifacts are excluded. Seven full-size exports use about
175 MiB; the upload before pruning briefly permits an eighth export. Manual runs
also count toward this limit and can shorten the number of calendar days retained.

Oversized exports fail instead of uploading a truncated dump or expanding the
storage budget. GitHub's account storage allowance still applies to this and
other repositories; a quota failure is reported by the upload step.

The hourly **Redis Command Budget** workflow also checks for a successful,
unexpired backup artifact younger than **30 hours**. It rejects pull-request
runs and backups from other branches. Export, restore, encryption, upload,
retention, and freshness failures use GitHub Actions failure notifications.
Confirm those notifications reach the owner's inbox.

Scheduled Actions runs can be delayed or disabled by GitHub. This is a logical
export with roughly daily recovery points, not managed point-in-time recovery.

## Recover into a separate database

Use an artifact from a successful **Database Backups** run on **main**. The receipt
has the encrypted filename, SHA-256, public recipient, and the result of the
pre-encryption dump restore check. Table counts and the dump checksum are inside
the encrypted manifest, not in the public receipt.

With age 1.3.2 and PostgreSQL 17/PostGIS 3.5 tools installed:

```bash
umask 077
jq -r '.encrypted_sha256 + "  " + .filename' receipt.json | sha256sum --check
age --decrypt --identity /secure/path/backup.agekey \
  --output database.tar /path/to/database-<timestamp>.tar.age
mkdir restore-files
tar -xf database.tar -C restore-files database.dump manifest.json
```

Provision an isolated PostgreSQL/PostGIS service. Set `RESTORE_ADMIN_URL` to its
administrative connection and `RESTORE_DATABASE_URL` to a **new** `ttl_recovery`
database on that service. These must not point at the serving production database.

```bash
createdb --maintenance-db "$RESTORE_ADMIN_URL" --template=template0 ttl_recovery
psql "$RESTORE_DATABASE_URL" -v ON_ERROR_STOP=1 -c 'DROP SCHEMA public'
pg_restore --exit-on-error --no-owner --no-privileges \
  --dbname "$RESTORE_DATABASE_URL" restore-files/database.dump
psql "$RESTORE_DATABASE_URL" -v ON_ERROR_STOP=1 \
  -c 'UPDATE public_cache_revision SET epoch=gen_random_uuid() WHERE singleton'
```

Verify successful SQLx migration history, table counts against `manifest.json`,
and the restored API's readiness and moderation visibility. Keep workers and
external writes disabled during the drill. Record the source artifact, restore
duration, and checks before considering any production switch. Remove plaintext
files and the disposable database after the drill. Follow
[the migration recovery procedure](MIGRATIONS.md) for an actual incident.

The workflow uses a PostgreSQL 17 dump client and refuses newer source servers.
Public-schema extension definitions are included explicitly because schema-only
selection otherwise omits dependencies such as PostGIS geography types. A target
missing an extension fails restoration and prevents publication.

To rotate keys, generate a new age identity, save it privately, and update only
the public recipient in the policy. Verify recovery with a fresh export. Keep
old private keys until all backups they protect have expired.
