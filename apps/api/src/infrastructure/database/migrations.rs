use sqlx::{PgPool, migrate::MigrateError};

/// Validate applied history and apply pending migrations under SQLx's database lock.
///
/// Ordinary startup rejects missing versions, changed checksums, and failed history.
/// `ignore_missing` is an explicit recovery override for missing versions only;
/// checksum mismatches and failed migrations still return an error. No listener or
/// worker should start until this succeeds.
pub async fn run_migrations(pool: &PgPool, ignore_missing: bool) -> Result<(), MigrateError> {
    if ignore_missing {
        tracing::warn!(
            "Migration recovery override enabled: applied versions absent from this binary will be ignored"
        );
    }
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(ignore_missing);
    migrator.run(pool).await
}
