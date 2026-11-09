# Contributing to VigilShell

This document defines the project's contribution, branch and version rules. Technical commands and packaging details live in [the technical stack](docs/TECH_STACK.md).

## Branches and pull requests

`main` is the shared integration branch. Use short-lived `feat/`, `fix/`, or `chore/` branches and target `main` with focused pull requests.

1. Inspect the current branch and worktree. Start new work from an up-to-date `main`. If changes already exist on `main`, preserve them on a short-lived branch before committing. Reuse an existing work branch when the task belongs to the same change.
2. Review and commit only the intended files. Use commit subjects such as `fix: correct release selection`, describing the resulting behavior.
3. Push the work branch and open a PR describing the change, relevant checks and remaining verification. A request to commit and push defaults to this work branch. Direct pushes to `main` require an explicitly authorized exception permitted by the repository's protection rules.
4. Merge after the current PR revision passes the required checks and review conversations are resolved. A solo maintainer may merge their own PR with zero required approving reviews. Local checks and hosted CI results must be reported separately.
5. Verify the resulting `main` CI run. Delete the short-lived branch after its intended changes are integrated and any remaining local work is preserved. For squash merges, verify the resulting changes as well as the PR's merged state.

For delegated work, commit and push authorization covers publishing the work branch. PR merge, release tags, Release publication and repository visibility changes require session authorization covering those actions. Reuse authorization already granted for the same scope.

Handle urgent fixes through a short-lived fix branch and the same PR checks. Revert a faulty integrated change through a PR when necessary. Shared history rewrites require explicit authorization.

Historical version baselines belong in immutable tags. Create a release preparation branch only when stabilization must proceed alongside new development. Create a maintenance branch from a released tag only when an older version line actually needs patches, and carry applicable fixes back to `main` through a PR. Routine development uses `main` and short-lived branches. Long-lived develop and per-major archive branches are outside this workflow.

## Checks and preview builds

Use the pinned Node, npm and Rust versions in `.nvmrc`, `package.json` and `rust-toolchain.toml`. Run `npm ci`, `npm run check` and the tests relevant to your change. Native work reuses VigilShell Dev; never test against production data. See [the technical stack](docs/TECH_STACK.md) for signing and build commands.

CI checks pull requests and pushes to `main`. Native, backend, dependency, packaging and CI changes also run an ARM64 macOS package and startup check with a disposable certificate. The aggregate check is named `CI verified`. Failed, pending or infrastructure-blocked required checks must be resolved before normal merging. PR and `main` CI use read-only repository permissions and disposable signing material.

Preview artifacts support isolated testing and use a temporary signing identity. Distributed packages use the fixed release identity.

For an explicit preview, run **Preview package** and optionally supply a full commit SHA. The selected commit must contain the preview tooling. Download the artifact matching its SHA, run ID and attempt; it contains a DMG, SHA-256 checksum, signing report and build manifest. Artifacts expire after 14 days and do not replace release backups.

## Versions and release boundaries

Source and release versions use stable `X.Y.Z` numbers. Prepare a version using `npm run version:prepare -- X.Y.Z`, review `package.json`, `package-lock.json`, `Cargo.toml` and `Cargo.lock`, then submit the change through a PR. Version preparation only updates those files. `npm run release:check` verifies their format and consistency.

Public version tags use `vX.Y.Z` and identify the exact release source commit. Create them on a verified `main` commit after the version change is integrated. Preserve published tags permanently. Corrections use a new version and tag. Releasing from a future maintenance branch requires explicit version-line source validation in the release workflow before that branch is eligible to publish.

Pushing a new `vX.Y.Z` tag starts **Release draft**. The workflow validates the tag, the four version files, `main` ancestry and a successful `main` push CI run for the exact source commit. The `macos-release` environment provides the fixed signing identity. The workflow verifies the ARM64 DMG and native startup, then attaches the installer, checksum, signing report and build manifest directly to a draft. Review the verified draft and its assets before publishing it manually.

An existing draft or published release blocks a second draft for that tag. If an upload stops partway through, the draft remains marked pending. Review and clear the incomplete draft before rerunning the original tag workflow. Keep the tag at the original commit.

Preview builds use the source version and are identified by commit SHA, run ID and attempt. Update discovery selects published stable releases with compatible installers and filters drafts and prereleases.

Keep release installers, checksums, build and signing reports, and securely stored signing material as release recovery assets. Git tags preserve source baselines. These separate assets support artifact verification and future signing continuity.

## GitHub protection requirements

The settings below define the required protection baseline. Verify the live GitHub settings and PR merge behavior before reporting these protections as enforced.

- Require PRs for `main`, with `CI verified` from GitHub Actions as a required status check and review conversations resolved before merging.
- Set required approving reviews to zero for solo maintenance. Apply normal merge checks to the maintainer as well as contributors.
- Protect `main` from force pushes and deletion, and protect release tags from updates and deletion.
- Keep the default Actions token read-only and require maintainer approval for external contributors' fork workflows. Fixed release signing material belongs in a restricted release environment.
- Configure release-tag protection and validate allowed sources before enabling tag-driven publication. Workflow files describe automation. Live repository settings establish enforcement.
