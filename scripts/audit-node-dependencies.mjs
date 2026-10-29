import { spawnSync } from "node:child_process";

/** Accept one reviewed, build-only advisory; any new version or path needs review. */
function isReviewedBuildFinding(advisory) {
  const expected = [
    "apps__web", // pnpm's audit report escapes workspace path separators.
    "vite-plugin-pwa",
    "workbox-build",
    "@rollup/plugin-node-resolve",
    "deepmerge",
  ];
  return (
    advisory.module_name === "deepmerge" &&
    advisory.severity === "low" &&
    advisory.url === "https://github.com/advisories/GHSA-gcqx-w48m-w2h4" &&
    advisory.findings?.length > 0 &&
    advisory.findings.every(
      (finding) =>
        finding.version === "4.3.1" &&
        finding.dev === true &&
        finding.paths?.length > 0 &&
        finding.paths.every(
          (path) =>
            path
              .split(/\s*>\s*/)
              .map((part) => part.replace(/@\d.*$/, ""))
              .join(">") === expected.join(">"),
        ),
    )
  );
}

const result = spawnSync("pnpm", ["audit", "--json"], {
  cwd: new URL("..", import.meta.url),
  encoding: "utf8",
  maxBuffer: 8 * 1024 * 1024,
});
if (result.error) throw result.error;
const report = JSON.parse(result.stdout);
const totals = report.metadata?.vulnerabilities;
if (
  report.error ||
  !totals ||
  !["info", "low", "moderate", "high", "critical"].every(
    (level) => Number.isInteger(totals[level]) && totals[level] >= 0,
  ) ||
  !report.advisories ||
  ![0, 1].includes(result.status)
) {
  throw new Error(
    `Dependency audit failed: ${JSON.stringify(report.error || result.stderr)}`,
  );
}
const findings = Object.values(report.advisories);
const count = Object.values(totals).reduce((sum, value) => sum + value, 0);
if (count > 0 !== findings.length > 0)
  throw new Error("Audit reported vulnerabilities without advisory details");
const unreviewed = findings.filter(
  (advisory) => !isReviewedBuildFinding(advisory),
);
for (const finding of unreviewed) {
  console.error(
    `${finding.severity}: ${finding.module_name} — ${finding.title} (${finding.url})`,
  );
}
if (unreviewed.length) process.exit(1);
if (findings.length) {
  console.warn(
    "One reviewed low-severity deepmerge advisory remains in PWA build tooling; see docs/DEPENDENCIES.md.",
  );
}
console.log("Dependency audit passed: no unreviewed advisories.");
