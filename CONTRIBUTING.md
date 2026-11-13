# Contributing to VigilShell

This document defines the project's contribution, branch and version rules. Technical commands and packaging details live in [the technical stack](docs/TECH_STACK.md).

## Branches and pull requests

`main` is the shared integration branch. Use short-lived `feat/`, `fix/`, or `chore/` branches and target `main` with focused pull requests.

1. Inspect the current branch and worktree. Start new work from an up-to-date `main`. If changes already exist on `main`, preserve them on a short-lived branch before committing. Reuse an existing work branch when the task belongs to the same change.
2. Review and commit only the intended files. Use commit subjects such as `fix: correct release selection`, describing the resulting behavior.
3. Push the work branch and open a PR describing the change, relevant checks and remaining verification. A request to commit and push defaults to this work branch. Direct pushes to `main` require an explicitly authorized exception permitted by the repository's protection rules.
4. Merge after the current PR revision passes the required checks and review conversations are resolved. A solo maintainer may merge their own PR with zero required approving reviews. Local checks and hosted CI results must be reported separately.
5. Verify the PR's merged state and the resulting changes on `main`. Delete the short-lived branch after its intended changes are integrated and any remaining local work is preserved.

For delegated work, commit and push authorization covers publishing the work branch. PR merge, release tags, Release publication and repository visibility changes require session authorization covering those actions. Reuse authorization already granted for the same scope.

Handle urgent fixes through a short-lived fix branch and the same PR checks. Revert a faulty integrated change through a PR when necessary. Shared history rewrites require explicit authorization.

Historical version baselines belong in immutable tags. Create a release preparation branch only when stabilization must proceed alongside new development. Create a maintenance branch from a released tag only when an older version line actually needs patches, and carry applicable fixes back to `main` through a PR. Routine development uses `main` and short-lived branches. Long-lived develop and per-major archive branches are outside this workflow.

## Checks and preview builds

Use the pinned Node, npm and Rust versions in `.nvmrc`, `package.json` and `rust-toolchain.toml`. Run `npm ci`, `npm run check` and the tests relevant to your change. Native work reuses VigilShell Dev; never test against production data. See [the technical stack](docs/TECH_STACK.md) for signing and build commands.

CI classifies changes in pull requests targeting `main`. Changes limited to `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `CHANGELOG.md`, Markdown under `docs/`, and `.jpg`, `.jpeg`, or `.png` images under `docs/images/` only run classification and the aggregate `CI verified` check. Other changes run frontend and backend checks. Failed, pending or infrastructure-blocked required checks must be resolved before normal merging. Run **CI** manually to check a selected ref with the full frontend and backend checks. CI uses read-only repository permissions.

Preview artifacts support isolated testing and use a temporary signing identity. Distributed packages use the fixed release identity.

For changes to native integrations, signing or installer generation, run **Preview package** manually for the exact commit before merging and record the installation or startup result in the PR. For other previews, the workflow uses the selected ref when no commit SHA is supplied. The selected commit must contain the preview tooling. Download the artifact matching its SHA, run ID and attempt. It contains a DMG, update ZIP, checksums, ZIP blockmap, update metadata, signing report and build manifest. Preview artifacts are available for 14 days.

## Versions and release boundaries

Source and release versions use stable `X.Y.Z` numbers. Prepare a version using `npm run version:prepare -- X.Y.Z`, review the resulting changes to `package.json`, `package-lock.json`, `Cargo.toml` and `Cargo.lock`, then submit those files, `docs/releases/vX.Y.Z.md` and the generated `CHANGELOG.md` through a PR. `npm run release:check` verifies version consistency. Run `npm run release:check -- --notes` on the candidate before creating its tag.

Public version tags use `vX.Y.Z` and identify the exact release source commit. Create them on a `main` commit after the version PR passes its checks and is merged. Preserve published tags permanently. Corrections use a new version and tag. Releasing from a future maintenance branch requires explicit version-line source validation in the release workflow before that branch is eligible to publish.

### Release notes

Write concise, user-facing notes in English for the tagged source. Use `## Features`, `## Bug Fixes`, and `## Upgrade Notes` for sections with content. Describe the visible effect of each change. Link to a translation on the same tag when one is available. For the first release, summarize the capabilities shipped in that installer.

Run `npm run changelog:generate` after editing version notes. [CHANGELOG.md](CHANGELOG.md) collects those notes in version order, newest first. Version note files are the source for the generated history. The release preflight checks that both are in sync.

Use the version number alone as the Release title in `X.Y.Z` format, for example `0.1.1`. The workflow renders the notes with those sections in order, followed by **Downloads**, **Installation & Upgrade**, and **Build & Verification**. The downloads table groups verified installers by OS and links each colored badge to its tag-specific asset. SHA-256 files are available in **Assets** for optional download verification. The changelog link below the table opens the cumulative history on `main`. Build details include a source comparison when there is an earlier published stable tag in the source history. Review the installer from the draft's **Assets** section. The direct links in the notes are for the published release.

### Draft and publication

Pushing a new `vX.Y.Z` tag starts **Release draft**. The workflow validates the tag, the four version files, the release notes and changelog, and `main` ancestry. It runs the full frontend and backend checks on the validated commit. Once both pass, the macOS job builds the same commit using the fixed signing identity from the `macos-release` environment. It verifies the ARM64 DMG, update ZIP, final metadata digests and native startup, then attaches both archives, their checksums, the ZIP blockmap, update metadata, signing report and build manifest directly to a draft. Review the generated notes and assets before publishing it manually.

Qualification uses only Node built-ins and does not require installed npm packages. Update-artifact dependencies are loaded only after the build jobs have installed the locked dependencies. New version tags continue to create drafts automatically; recovery is not part of the normal release path.

If a workflow defect prevents a draft from being created, fix and test the orchestration through a separate PR. Maintainers may then run **Release draft** manually from `main`, supplying the existing stable tag and its full expected product commit SHA. The control checkout is pinned to the dispatch commit; the product checkout remains pinned to the existing immutable tag commit. Recovery validates both identities, clean trees, version notes, main ancestry and unused tag before signing or publishing a draft. It runs the full frontend/backend checks and fixed-identity native packaging validations on the original product source. The build manifest records the product SHA/tree and the recovery control SHA alongside the actual run ID. Do not move tags, edit the product checkout, override event identity, reuse preview packages, or weaken signing and digest checks to recover a release. Product changes require a new version and tag.

An existing draft or published release blocks a second draft for that tag. If an upload stops partway through, the draft remains marked pending. Review and clear the incomplete draft before rerunning the original tag workflow. Keep the tag at the original commit.

Preview builds use the source version and are identified by commit SHA, run ID and attempt. Update discovery selects published stable releases with compatible installers and filters drafts and prereleases.

Keep release installers, checksums, build and signing reports, and securely stored signing material as release recovery assets. Git tags preserve source baselines. These separate assets support artifact verification and future signing continuity.

## GitHub protection requirements

The settings below define the required protection baseline. Verify the live GitHub settings and PR merge behavior before reporting these protections as enforced.

- Require PRs for `main`, with branches up to date, `CI verified` from GitHub Actions passing, and review conversations resolved before merging.
- Set required approving reviews to zero for solo maintenance. Apply normal merge checks to the maintainer as well as contributors.
- Protect `main` from force pushes and deletion, and protect release tags from updates and deletion.
- Keep the default Actions token read-only and require maintainer approval for external contributors' fork workflows. Fixed release signing material belongs in a restricted release environment.
- Configure release-tag protection and validate allowed sources before enabling tag-driven publication. Workflow files describe automation. Live repository settings establish enforcement.
