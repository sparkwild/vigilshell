import assert from 'node:assert/strict';
import { isStableReleaseTag } from './release-policy.mjs';

// Recovery changes the orchestrator, never the immutable product source.
export function releaseContext(env) {
  if (env.GITHUB_EVENT_NAME === 'push') {
    assert.equal(env.GITHUB_REF_TYPE, 'tag');
    assert(env.GITHUB_REF.startsWith('refs/tags/'));
    const tag = env.GITHUB_REF.slice('refs/tags/'.length);
    assert(isStableReleaseTag(tag));
    return { tag, recovery: false };
  }
  assert.equal(env.GITHUB_EVENT_NAME, 'workflow_dispatch');
  assert.equal(env.GITHUB_REF, 'refs/heads/main', 'Recovery must run from main');
  assert.equal(env.GITHUB_REF_TYPE, 'branch');
  assert.equal(
    env.GITHUB_WORKFLOW_REF,
    `${env.GITHUB_REPOSITORY}/.github/workflows/release-draft.yml@refs/heads/main`,
  );
  assert(isStableReleaseTag(env.RECOVERY_TAG ?? ''), 'Expected a stable recovery tag');
  assert(/^[a-f0-9]{40}$/.test(env.RECOVERY_SHA ?? ''), 'Expected a full product commit SHA');
  assert(/^[a-f0-9]{40}$/.test(env.GITHUB_SHA ?? ''), 'Expected a full control commit SHA');
  return {
    tag: env.RECOVERY_TAG,
    expectedSha: env.RECOVERY_SHA,
    recovery: true,
    controlSha: env.GITHUB_SHA,
  };
}
