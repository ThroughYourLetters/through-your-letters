import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import {
  closeSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import policy from "../ops/database-backup.json" with { type: "json" };

function run(program, args, { stdout = "pipe", timeout = 300000 } = {}) {
  const result = spawnSync(program, args, {
    env: {
      PATH: process.env.PATH,
      HOME: process.env.HOME,
      LANG: "C.UTF-8",
      PGCONNECT_TIMEOUT: "10",
      PGAPPNAME: "through-your-letters-backup",
    },
    encoding: "utf8",
    stdio: ["ignore", stdout, "pipe"],
    timeout,
    maxBuffer: 1024 * 1024,
  });
  if (result.status !== 0 || result.error) {
    // Provider errors may contain connection details or application data.
    throw new Error(`${program} failed; no backup was published`);
  }
  return result.stdout?.trim();
}

function databaseUrl(value) {
  try {
    const url = new URL(value);
    if (!["postgres:", "postgresql:"].includes(url.protocol) || !url.hostname)
      throw new Error();
    return url;
  } catch {
    throw new Error("A valid PostgreSQL connection URL is required");
  }
}

function restoreUrl(value) {
  const url = databaseUrl(value);
  if (!["localhost", "127.0.0.1", "[::1]", "restore"].includes(url.hostname)) {
    throw new Error(
      "Restore verification must use a local isolated PostgreSQL service",
    );
  }
  return url;
}

function sql(url, statement) {
  return run("psql", [
    "-X",
    "--quiet",
    "--no-psqlrc",
    "--set=ON_ERROR_STOP=1",
    "--tuples-only",
    "--no-align",
    "--dbname",
    url.href,
    "--command",
    statement,
  ]);
}

export async function temporaryDatabase(baseValue, operation) {
  const base = restoreUrl(baseValue);
  const name = `ttl_backup_verify_${randomUUID().replaceAll("-", "")}`;
  run("createdb", [
    "--maintenance-db",
    base.href,
    "--template=template0",
    name,
  ]);
  const target = new URL(base);
  target.pathname = `/${name}`;
  try {
    return await operation(target);
  } finally {
    run("dropdb", ["--maintenance-db", base.href, "--force", name], {
      timeout: 30000,
    });
  }
}

export async function verifyDump(dump, restoreBase) {
  return temporaryDatabase(restoreBase, (target) => {
    // The archive recreates public; this newly created database starts with it empty.
    sql(target, "DROP SCHEMA public");
    run("pg_restore", [
      "--exit-on-error",
      "--no-owner",
      "--no-privileges",
      "--dbname",
      target.href,
      dump,
    ]);
    const result = JSON.parse(
      sql(
        target,
        `SELECT json_build_object(
      'migrations', (SELECT COUNT(*) FROM public._sqlx_migrations WHERE success),
      'failed_migrations', (SELECT COUNT(*) FROM public._sqlx_migrations WHERE NOT success),
      'users', (SELECT COUNT(*) FROM public.users),
      'letterings', (SELECT COUNT(*) FROM public.letterings),
      'hidden_letterings', (SELECT COUNT(*) FROM public.letterings WHERE status::text IN ('PENDING','REJECTED')),
      'epoch_rows', (SELECT COUNT(*) FROM public.public_cache_revision WHERE singleton),
      'writers', (SELECT COUNT(*) FROM public.public_cache_writers))`,
      ),
    );
    if (
      result.migrations < 1 ||
      result.failed_migrations !== 0 ||
      result.epoch_rows !== 1
    ) {
      throw new Error(
        "Restored migration history or cache freshness state is invalid",
      );
    }
    const before = sql(
      target,
      "SELECT epoch FROM public.public_cache_revision WHERE singleton",
    );
    const after = sql(
      target,
      "UPDATE public.public_cache_revision SET epoch=gen_random_uuid() WHERE singleton RETURNING epoch",
    );
    if (before === after)
      throw new Error("Restored cache epoch did not rotate");
    return result;
  });
}

export async function createDatabaseBackup({
  sourceUrl,
  restoreBase,
  outputDirectory,
  recipient = policy.recipient,
  maxBytes = policy.max_archive_bytes,
  allowLocalSource = false,
}) {
  process.umask(0o077);
  restoreUrl(restoreBase);
  const source = databaseUrl(sourceUrl);
  const local = ["localhost", "127.0.0.1", "[::1]", "restore"].includes(
    source.hostname,
  );
  if (!(allowLocalSource && local)) {
    const ssl = source.searchParams.get("sslmode") ?? "require";
    if (!["require", "verify-ca", "verify-full"].includes(ssl)) {
      throw new Error("Remote database backups require TLS");
    }
    source.searchParams.set("sslmode", ssl);
  }
  if (
    source.hostname.endsWith(".pooler.supabase.com") &&
    source.port === "6543"
  ) {
    throw new Error("Use the Supabase session pooler on port 5432 for backups");
  }
  if (
    !Number.isSafeInteger(maxBytes) ||
    maxBytes < 1024 ||
    maxBytes > 25 * 1024 * 1024
  ) {
    throw new Error("Backup archive limit must be between 1 KiB and 25 MiB");
  }
  if (!recipient || typeof recipient !== "string")
    throw new Error("Set a backup encryption recipient");
  const serverMajor = Math.floor(
    Number(sql(source, "SHOW server_version_num")) / 10000,
  );
  const clientMajor = Number(
    run("pg_dump", ["--version"]).match(/PostgreSQL\) (\d+)/)?.[1],
  );
  if (!serverMajor || !clientMajor || serverMajor > clientMajor) {
    throw new Error(
      "The PostgreSQL dump client must support the source server version",
    );
  }
  const extensions = JSON.parse(
    sql(
      source,
      "SELECT COALESCE(json_agg(extname ORDER BY extname),'[]'::json) FROM pg_extension WHERE extnamespace='public'::regnamespace",
    ),
  );
  if (
    !Array.isArray(extensions) ||
    extensions.some((name) => !/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(name))
  ) {
    throw new Error("Public extension names could not be validated");
  }
  const output = resolve(outputDirectory);
  mkdirSync(output, { recursive: true, mode: 0o700 });
  if (readdirSync(output).length)
    throw new Error("Backup output directory must be empty");
  const working = mkdtempSync(join(tmpdir(), "ttl-backup-"));
  try {
    const started = Date.now();
    const dump = join(working, "database.dump");
    const descriptor = openSync(dump, "wx", 0o600);
    try {
      run(
        "pg_dump",
        [
          "--dbname",
          source.href,
          "--format=custom",
          "--compress=6",
          "--schema=public",
          ...extensions.map((name) => `--extension=${name}`),
          "--no-owner",
          "--no-privileges",
          "--lock-wait-timeout=5s",
        ],
        { stdout: descriptor },
      );
    } finally {
      closeSync(descriptor);
    }
    if (statSync(dump).size > maxBytes)
      throw new Error("Database dump exceeds the free backup storage budget");
    const verification = await verifyDump(dump, restoreBase);
    const createdAt = new Date().toISOString();
    const manifest = {
      format_version: 1,
      created_at: createdAt,
      schema: "public",
      source_pg_major: serverMajor,
      dump_sha256: createHash("sha256")
        .update(readFileSync(dump))
        .digest("hex"),
      verified_restore: verification,
    };
    writeFileSync(join(working, "manifest.json"), JSON.stringify(manifest), {
      mode: 0o600,
    });
    const bundle = join(working, "database.tar");
    run("tar", [
      "-cf",
      bundle,
      "-C",
      working,
      "database.dump",
      "manifest.json",
    ]);
    const encrypted = join(working, "database.tar.age");
    run("age", [
      "--encrypt",
      "--recipient",
      recipient,
      "--output",
      encrypted,
      bundle,
    ]);
    const size = statSync(encrypted).size;
    if (size > maxBytes)
      throw new Error(
        "Encrypted archive exceeds the free backup storage budget",
      );
    const filename = `database-${createdAt.replaceAll(/[:.]/g, "-")}.tar.age`;
    const receipt = {
      format_version: 1,
      created_at: createdAt,
      filename,
      encrypted_bytes: size,
      encrypted_sha256: createHash("sha256")
        .update(readFileSync(encrypted))
        .digest("hex"),
      recipient,
      dump_restore_verified: true,
      elapsed_ms: Date.now() - started,
    };
    copyFileSync(encrypted, join(output, filename));
    writeFileSync(
      join(output, "receipt.json"),
      `${JSON.stringify(receipt, null, 2)}\n`,
      { mode: 0o600 },
    );
    return receipt;
  } finally {
    rmSync(working, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  try {
    if (process.argv[2] === "policy") {
      console.log(`retention_days=${policy.retention_days}`);
    } else {
      if (!process.env.DATABASE_BACKUP_URL) {
        throw new Error(
          "Add the DATABASE_BACKUP_URL repository Actions secret before activating backups",
        );
      }
      console.log(
        JSON.stringify(
          await createDatabaseBackup({
            sourceUrl: process.env.DATABASE_BACKUP_URL,
            restoreBase: process.env.BACKUP_VERIFY_URL,
            outputDirectory: "database-backups",
          }),
        ),
      );
    }
  } catch (error) {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
  }
}
