const assert = require("node:assert/strict");
const { mkdtemp, writeFile, readFile, rm } = require("node:fs/promises");
const { createRequire } = require("node:module");
const { tmpdir } = require("node:os");
const { dirname, join } = require("node:path");

/** Verify Capacitor's real template extractor against the patched archive dependency. */
async function main() {
  const cli = dirname(require.resolve("@capacitor/cli/package.json"));
  const native = createRequire(join(cli, "package.json"));
  const tar = native("tar");
  const { extractTemplate } = native("./dist/util/template.js");
  const directory = await mkdtemp(join(tmpdir(), "ttl-native-template-"));
  try {
    await writeFile(join(directory, "fixture.txt"), "template payload");
    await tar.create(
      { cwd: directory, file: join(directory, "fixture.tgz"), gzip: true },
      ["fixture.txt"],
    );
    await extractTemplate(
      join(directory, "fixture.tgz"),
      join(directory, "output"),
    );
    assert.equal(
      await readFile(join(directory, "output/fixture.txt"), "utf8"),
      "template payload",
    );
    console.log("Capacitor template extraction passed with patched tar.");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
