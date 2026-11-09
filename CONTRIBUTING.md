# Contributing to VigilShell

Target `main` with focused pull requests from short-lived `feat/`, `fix/`, or `chore/` branches. Describe the behavior change, relevant checks and any remaining verification. Maintainers can merge their own pull requests after checks; another reviewer is not mandatory for solo maintenance.

Use the pinned Node, npm and Rust versions in `.nvmrc`, `package.json` and `rust-toolchain.toml`. Run `npm ci`, `npm run check` and the tests relevant to your change. Native work reuses VigilShell Dev; never test against production data. See [the technical stack](docs/TECH_STACK.md) for signing and build commands.

CI checks pull requests and pushes to main. Native, backend, dependency, packaging and CI changes also run an ARM64 macOS package and startup check with a disposable certificate. Preview artifacts are for isolated testing, not distribution: their temporary signing identity is not the fixed release identity. No CI workflow creates or publishes a GitHub Release.

For an explicit preview, run **Preview package** and optionally supply a full commit SHA. The selected commit must contain the preview tooling. Download the artifact matching its SHA, run ID and attempt; it contains a DMG, SHA-256 checksum, signing report and build manifest. Artifacts expire after 14 days and do not replace release backups.

Prepare a version using `npm run version:prepare -- X.Y.Z`, review the four version files, then commit normally. This command does not create tags or publish anything. `npm run release:check` requires a stable `X.Y.Z` version consistently across all four files. Public version tags use `vX.Y.Z` and must never move or be reused. Update discovery selects published stable releases with compatible installers. Preview builds use the source version and are identified by commit SHA, run ID and attempt. Tag-driven release drafting is not implemented by the current workflows.

Historical release baselines belong in tags. Create a release preparation branch only when stabilization must proceed alongside new development; create a maintenance branch only when an older line actually needs patches. Do not keep a permanent develop branch or an archive branch for every major version as part of normal development. Retire migration branches only after all intended changes are integrated.
