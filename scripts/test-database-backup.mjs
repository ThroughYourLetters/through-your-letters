import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import {
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import {
  createDatabaseBackup,
  temporaryDatabase,
  verifyDump,
} from "./database-backup.mjs";

const base = process.env.BACKUP_TEST_URL;
assert.ok(base, "Set BACKUP_TEST_URL to a disposable local PostgreSQL service");
const work = mkdtempSync(join(tmpdir(), "ttl-backup-test-"));
const execute = (name, args) =>
  execFileSync(name, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
const sql = (url, statement) =>
  execute("psql", [
    "-XqAt",
    "--set=ON_ERROR_STOP=1",
    "--dbname",
    url.href,
    "--command",
    statement,
  ]);
try {
  const identity = join(work, "recovery.agekey");
  execute("age-keygen", ["-o", identity]);
  const recipient = execute("age-keygen", ["-y", identity]);
  await temporaryDatabase(base, async (source) => {
    sql(
      source,
      "CREATE TABLE _sqlx_migrations(version bigint PRIMARY KEY, description text NOT NULL, installed_on timestamptz NOT NULL DEFAULT now(), success boolean NOT NULL, checksum bytea NOT NULL, execution_time bigint NOT NULL)",
    );
    const migrations = resolve("apps/api/migrations");
    for (const file of readdirSync(migrations)
      .filter((name) => /^\d+_.+\.sql$/.test(name))
      .sort()) {
      execute("psql", [
        "-Xq",
        "--set=ON_ERROR_STOP=1",
        "--dbname",
        source.href,
        "--file",
        join(migrations, file),
      ]);
      const version = file.split("_")[0];
      const checksum = createHash("sha384")
        .update(readFileSync(join(migrations, file)))
        .digest("hex");
      sql(
        source,
        `INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES(${version},'backup fixture',true,decode('${checksum}','hex'),0)`,
      );
    }
    const user = randomUUID();
    const image = randomUUID();
    const marker = `private-backup-fixture-${randomUUID()}`;
    sql(
      source,
      `INSERT INTO users(id,email,password_hash,role) VALUES('${user}','backup@example.invalid','${marker}','USER')`,
    );
    sql(
      source,
      `INSERT INTO letterings(id,city_id,user_id,contributor_tag,image_url,thumbnail_small,thumbnail_medium,thumbnail_large,pin_code,status) VALUES('${image}',(SELECT id FROM cities LIMIT 1),'${user}','Backup fixture','image','small','medium','large','560025','REJECTED')`,
    );
    const receipt = await createDatabaseBackup({
      sourceUrl: source.href,
      restoreBase: base,
      recipient,
      outputDirectory: join(work, "published"),
      allowLocalSource: true,
    });
    assert.equal(receipt.dump_restore_verified, true);
    const files = readdirSync(join(work, "published"));
    assert.deepEqual(files.sort(), [receipt.filename, "receipt.json"].sort());
    const encrypted = join(work, "published", receipt.filename);
    assert.equal(readFileSync(encrypted).includes(Buffer.from(marker)), false);
    assert.equal(
      createHash("sha256").update(readFileSync(encrypted)).digest("hex"),
      receipt.encrypted_sha256,
    );
    const decrypted = join(work, "database.tar");
    execute("age", [
      "--decrypt",
      "--identity",
      identity,
      "--output",
      decrypted,
      encrypted,
    ]);
    execute("tar", ["-xf", decrypted, "-C", work]);
    const manifest = JSON.parse(
      readFileSync(join(work, "manifest.json"), "utf8"),
    );
    assert.equal(manifest.verified_restore.hidden_letterings, 1);
    assert.ok(manifest.verified_restore.migrations > 0);
    assert.equal(
      manifest.dump_sha256,
      createHash("sha256")
        .update(readFileSync(join(work, "database.dump")))
        .digest("hex"),
    );
    assert.deepEqual(
      await verifyDump(join(work, "database.dump"), base),
      manifest.verified_restore,
    );
    const wrongIdentity = join(work, "wrong.agekey");
    execute("age-keygen", ["-o", wrongIdentity]);
    assert.throws(() =>
      execFileSync(
        "age",
        ["--decrypt", "--identity", wrongIdentity, encrypted],
        { stdio: ["ignore", "ignore", "pipe"] },
      ),
    );
    const corrupted = readFileSync(encrypted);
    corrupted[corrupted.length - 1] ^= 1;
    const damaged = join(work, "damaged.age");
    writeFileSync(damaged, corrupted);
    assert.throws(() =>
      execFileSync("age", ["--decrypt", "--identity", identity, damaged], {
        stdio: ["ignore", "ignore", "pipe"],
      }),
    );
    await assert.rejects(
      createDatabaseBackup({
        sourceUrl: source.href,
        restoreBase: base,
        recipient,
        outputDirectory: join(work, "too-large"),
        maxBytes: 1024,
        allowLocalSource: true,
      }),
      /storage budget/,
    );
    assert.deepEqual(readdirSync(join(work, "too-large")), []);
    await assert.rejects(
      createDatabaseBackup({
        sourceUrl: source.href,
        restoreBase: "postgresql://production.example/database",
        recipient,
        outputDirectory: join(work, "unsafe"),
      }),
      /local isolated/,
    );
    await assert.rejects(
      createDatabaseBackup({
        sourceUrl:
          "postgresql://user:must-not-log@production.example/db?sslmode=disable",
        restoreBase: base,
        recipient,
        outputDirectory: join(work, "insecure"),
      }),
      /require TLS/,
    );
  });
  console.log(
    "PASS: actual migrations, encrypted export, independent decrypt/restore, hidden data, wrong keys, corruption, size limits, and safe restore targets",
  );
} finally {
  rmSync(work, { recursive: true, force: true });
}
