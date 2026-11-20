import { spawnSync } from "node:child_process";

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
for (const finding of findings) {
  console.error(
    `${finding.severity}: ${finding.module_name} — ${finding.title} (${finding.url})`,
  );
}
if (findings.length) process.exit(1);
console.log("Dependency audit passed: no reported vulnerabilities.");
