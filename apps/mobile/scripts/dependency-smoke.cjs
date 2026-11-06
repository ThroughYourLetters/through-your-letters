const assert = require("node:assert/strict");
const {
  mkdtemp,
  writeFile,
  readFile,
  rm,
  access,
} = require("node:fs/promises");
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
    await writeFile(join(directory, "remove-a.txt"), "temporary native file");
    await writeFile(join(directory, "keep.txt"), "retained native file");
    await native("rimraf").rimraf(join(directory, "remove-*.txt"), {
      glob: true,
    });
    await assert.rejects(access(join(directory, "remove-a.txt")), {
      code: "ENOENT",
    });
    assert.equal(
      await readFile(join(directory, "keep.txt"), "utf8"),
      "retained native file",
    );
    console.log("Capacitor cleanup passed with patched glob matching.");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
