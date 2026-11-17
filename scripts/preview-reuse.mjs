import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { closeSync, existsSync, mkdirSync, openSync, unlinkSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const arches = ['arm64', 'x64'];
export function selectPreviewArtifacts(artifacts, sha, runId, attempt) {
  assert.match(sha, /^[0-9a-f]{40}$/);
  for (const value of [runId, attempt]) assert.match(value, /^[1-9][0-9]*$/);
  const selected = {};
  for (const arch of arches) {
    const prefix = `preview-macos-${arch}-${sha}-${runId}-`;
    const candidates = artifacts
      .filter((artifact) => {
        if (!artifact.name.startsWith(prefix)) return false;
        const suffix = artifact.name.slice(prefix.length);
        assert.match(suffix, /^[1-9][0-9]*$/);
        return BigInt(suffix) <= BigInt(attempt);
      })
      .sort((a, b) =>
        Number(BigInt(b.name.slice(prefix.length)) - BigInt(a.name.slice(prefix.length))),
      );
    assert(candidates.length, `Missing verified ${arch} artifact in this run`);
    const artifact = candidates[0];
    const buildAttempt = artifact.name.slice(prefix.length);
    assert.equal(
      candidates.filter((item) => item.name === artifact.name).length,
      1,
      'Ambiguous artifact',
    );
    assert.equal(
      artifact.expired,
      false,
      'Latest verified artifact expired; do not silently use an older build',
    );
    assert(Number.isSafeInteger(artifact.id) && artifact.id > 0);
    assert.equal(String(artifact.workflow_run?.id), runId);
    selected[arch] = { id: artifact.id, name: artifact.name, attempt: buildAttempt };
  }
  return selected;
}

export function assertSuccessfulPreviewJob(jobs, arch, attempt, runId) {
  assert(arches.includes(arch));
  const matching = jobs.filter(
    (job) => job.name === `package-${arch}` || job.name.startsWith(`package (${arch},`),
  );
  assert.equal(matching.length, 1, `Missing or ambiguous ${arch} package job`);
  const job = matching[0];
  assert.equal(String(job.run_id), runId);
  assert.equal(String(job.run_attempt), attempt);
  assert.equal(job.status, 'completed');
  assert.equal(job.conclusion, 'success', `Selected ${arch} package job did not succeed`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const {
    GITHUB_REPOSITORY: repo,
    GITHUB_RUN_ID: runId,
    GITHUB_RUN_ATTEMPT: attempt,
  } = process.env;
  assert.match(repo, /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/);
  for (const value of [runId, attempt]) assert.match(value, /^[1-9][0-9]*$/);
  const gh = (...args) => execFileSync('gh', args, { encoding: 'utf8' });
  const pages = (endpoint) => JSON.parse(gh('api', '--paginate', '--slurp', endpoint));
  const sha = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  const selected = selectPreviewArtifacts(
    pages(`repos/${repo}/actions/runs/${runId}/artifacts?per_page=100`).flatMap(
      (page) => page.artifacts,
    ),
    sha,
    runId,
    attempt,
  );
  // Validate every selected job before downloading anything. No cross-run lookup/fallback.
  const jobs = new Map();
  for (const arch of arches) {
    const item = selected[arch];
    if (!jobs.has(item.attempt))
      jobs.set(
        item.attempt,
        pages(
          `repos/${repo}/actions/runs/${runId}/attempts/${item.attempt}/jobs?per_page=100`,
        ).flatMap((page) => page.jobs),
      );
    assertSuccessfulPreviewJob(jobs.get(item.attempt), arch, item.attempt, runId);
  }
  const input = '.cache/preview-builds';
  assert(!existsSync(input), 'Refusing to mix with pre-existing downloads');
  mkdirSync(input, { recursive: true });
  for (const arch of arches) {
    const archive = `${input}/${arch}.zip`;
    const fd = openSync(archive, 'wx', 0o600);
    try {
      // Download the selected immutable ID, never resolve the mutable name again.
      execFileSync('gh', ['api', `repos/${repo}/actions/artifacts/${selected[arch].id}/zip`], {
        stdio: ['ignore', fd, 'pipe'],
      });
    } finally {
      closeSync(fd);
    }
    // Our upload is flat. Reject traversal, links and duplicate entries before extraction.
    execFileSync(
      'python3',
      [
        '-c',
        `
import os, re, stat, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as archive:
    entries = archive.infolist()
    names = [entry.filename for entry in entries]
    assert names and len(names) == len(set(names)), 'Duplicate or empty archive'
    for entry in entries:
        assert re.fullmatch(r'[A-Za-z0-9_][A-Za-z0-9_.-]*', entry.filename), 'Non-flat artifact'
        mode = entry.external_attr >> 16
        assert stat.S_IFMT(mode) in (0, stat.S_IFREG), 'Non-regular artifact entry'
    os.mkdir(sys.argv[2])
    archive.extractall(sys.argv[2])
`,
        archive,
        `${input}/${arch}`,
      ],
      { stdio: ['ignore', 'pipe', 'pipe'] },
    );
    unlinkSync(archive);
  }
  writeFileSync(
    `${input}/selection.json`,
    JSON.stringify({ sha, runId, aggregationAttempt: attempt, artifacts: selected }, null, 2) +
      '\n',
  );
}
