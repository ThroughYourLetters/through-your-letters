import policy from "../ops/database-backup.json" with { type: "json" };

function backupApi({ repository, token, request = fetch }) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? "") || !token) {
    throw new Error("Set GITHUB_REPOSITORY and GH_TOKEN for backup monitoring");
  }
  async function get(path, method = "GET") {
    let response;
    try {
      response = await request(
        `https://api.github.com/repos/${repository}/${path}`,
        {
          method,
          headers: {
            Authorization: `Bearer ${token}`,
            Accept: "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
          },
          redirect: "error",
          signal: AbortSignal.timeout(10000),
        },
      );
    } catch {
      throw new Error("Backup monitoring request failed or timed out");
    }
    if (!response.ok)
      throw new Error(`Backup monitoring returned HTTP ${response.status}`);
    if (response.status === 204) return null;
    try {
      return await response.json();
    } catch {
      throw new Error("Backup monitoring response is not JSON");
    }
  }
  return get;
}

export async function checkBackupFreshness({
  repository,
  token,
  now = new Date(),
  request = fetch,
}) {
  const get = backupApi({ repository, token, request });
  const result = await get(
    "actions/workflows/database-backup.yml/runs?branch=main&status=success&per_page=20",
  );
  if (!Array.isArray(result?.workflow_runs))
    throw new Error("Backup workflow response has no run list");
  const runs = result.workflow_runs
    .filter(
      (run) =>
        Number.isSafeInteger(run.id) &&
        run.head_branch === "main" &&
        run.conclusion === "success" &&
        ["schedule", "workflow_dispatch"].includes(run.event),
    )
    .slice(0, 5);
  for (const run of runs) {
    const artifacts = await get(
      `actions/runs/${run.id}/artifacts?per_page=100`,
    );
    if (!Array.isArray(artifacts?.artifacts))
      throw new Error("Backup artifact response has no artifact list");
    const artifact = artifacts.artifacts.find((item) => {
      const age = now.getTime() - Date.parse(item.created_at);
      return (
        /^database-backup-\d+-\d+$/.test(item.name ?? "") &&
        item.expired === false &&
        Number.isSafeInteger(item.size_in_bytes) &&
        item.size_in_bytes > 0 &&
        age >= -300000 &&
        age <= policy.max_age_hours * 3600000 &&
        Date.parse(item.expires_at) > now.getTime()
      );
    });
    if (artifact)
      return {
        checked_at: now.toISOString(),
        backup_run_id: run.id,
        artifact_created_at: artifact.created_at,
        age_hours: Math.max(
          0,
          (now.getTime() - Date.parse(artifact.created_at)) / 3600000,
        ),
      };
  }
  throw new Error(
    `No successful retained database backup from the last ${policy.max_age_hours} hours`,
  );
}

export async function pruneDatabaseBackups({
  repository,
  token,
  currentArtifactId,
  request = fetch,
}) {
  const api = backupApi({ repository, token, request });
  if (
    !Number.isSafeInteger(currentArtifactId) ||
    currentArtifactId < 1 ||
    !Number.isSafeInteger(policy.max_artifacts) ||
    policy.max_artifacts < 1 ||
    policy.max_artifacts > 7
  ) {
    throw new Error(
      "A new backup artifact and retention limit of 1..7 are required",
    );
  }
  const current = await api(`actions/artifacts/${currentArtifactId}`);
  const isBackup = (item) => /^database-backup-\d+-\d+$/.test(item?.name ?? "");
  if (
    current?.id !== currentArtifactId ||
    !isBackup(current) ||
    current.expired !== false ||
    !Number.isSafeInteger(current.size_in_bytes) ||
    current.size_in_bytes < 1
  ) {
    throw new Error(
      "The new retained backup must exist before pruning older backups",
    );
  }
  const backups = [];
  for (let page = 1; page <= 10; page++) {
    const result = await api(`actions/artifacts?per_page=100&page=${page}`);
    if (!Array.isArray(result?.artifacts))
      throw new Error("Backup artifact response has no artifact list");
    backups.push(...result.artifacts.filter(isBackup));
    if (result.artifacts.length < 100) break;
    if (page === 10)
      throw new Error(
        "Artifact listing exceeded the safe pruning limit; nothing was deleted",
      );
  }
  if (
    backups.some(
      (item) =>
        !Number.isSafeInteger(item.id) ||
        !Number.isFinite(Date.parse(item.created_at)),
    )
  ) {
    throw new Error("Backup artifact metadata is invalid; nothing was deleted");
  }
  backups.sort(
    (left, right) =>
      Date.parse(right.created_at) - Date.parse(left.created_at) ||
      right.id - left.id,
  );
  if (
    !backups
      .slice(0, policy.max_artifacts)
      .some((item) => item.id === currentArtifactId)
  ) {
    throw new Error(
      "New backup was not found among the retained artifacts; nothing was deleted",
    );
  }
  const expired = backups.slice(policy.max_artifacts);
  for (const artifact of expired)
    await api(`actions/artifacts/${artifact.id}`, "DELETE");
  return {
    retained: Math.min(backups.length, policy.max_artifacts),
    deleted: expired.length,
  };
}

if (import.meta.main) {
  try {
    const options = {
      repository: process.env.GITHUB_REPOSITORY,
      token: process.env.GH_TOKEN,
    };
    console.log(
      JSON.stringify(
        process.argv[2] === "prune"
          ? await pruneDatabaseBackups({
              ...options,
              currentArtifactId: Number(process.env.LATEST_BACKUP_ARTIFACT_ID),
            })
          : await checkBackupFreshness(options),
      ),
    );
  } catch (error) {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
  }
}
