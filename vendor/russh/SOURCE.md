# Source Provenance

## `russh` 0.62.6

- Registry source: crates.io package `russh` version `0.62.6`, expanded from
  `/Users/zhangjie/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/russh-0.62.6`.
- Crate archive SHA-256:
  `b41043523e0edcbd4e31d00903e26f12994f63b21bae9904f7405c1ed92752a5`.
- Upstream repository: `https://github.com/warp-tech/russh`.
- Upstream commit recorded by Cargo:
  `27554ed84c9af98012bf3c8cb4af2a36b0f6bdec` (`path_in_vcs = "russh"`).
- License metadata: Apache-2.0. The published crate did not contain a standalone
  license or notice file; this fork includes `LICENSE-APACHE` with the Apache
  License 2.0 text.

The complete expanded crate was copied before the changes listed in
`PATCHES.md`. Cargo's `.cargo-ok` and `.cargo_vcs_info.json` source metadata,
README, examples, benchmarks, tests, and original-form manifest are retained.
The crate-archive hash above identifies the immutable upstream input; it is not
a hash of this patched directory.

## Nested `ssh-key` 0.7.0-rc.11

- Registry source: crates.io package `ssh-key` version `0.7.0-rc.11`, expanded
  from the same local crates.io registry.
- Crate archive SHA-256:
  `f9a32fae177b74a22aa9c5b01bf7e68b33545be32d9e381e248058d2adc15ce3`.
- Upstream repository: `https://github.com/RustCrypto/SSH`.
- Upstream commit recorded by Cargo:
  `7a1c757d2e2fcde32680e373d5226481de6d434e` (`path_in_vcs = "ssh-key"`).
- License metadata and retained files: Apache-2.0 OR MIT,
  `LICENSE-APACHE`, and `LICENSE-MIT`.

This exact dependency is nested at `dependencies/ssh-key` so the product fork
can remove the unused DSA dependency and feature surface. Cargo resolves every
declared optional dependency even when its feature is disabled; the stock
manifest therefore could not be resolved from the approved offline cache after
DSA was excluded only at the `russh` feature layer.

The path dependency also disables `ssh-key` defaults and does not enable its
private-key `encryption` or `ppk` features. The selected production surface is
`std`, Ed25519, P-256, P-384, P-521, SHA-1 parsing support, and RSA when
`russh/rsa` is selected. The nested `ssh-key` manifest disables defaults on its
`ssh-cipher` dependency as an additional feature-unification guard.

## Cargo Resolution

The repository root forces crates.io `russh` references to this directory with:

```toml
[patch.crates-io]
russh = { path = "vendor/russh" }
```

The vendored manifest declares a standalone workspace for isolated offline
verification without connecting it to a product transport crate. Its
`Cargo.lock` is deliberately retained as the reproducible lock for that scoped
verification and was generated using only the local Cargo cache. Dependency
surface verification must also run from the repository root against
`vigilshell-transport-russh`; a standalone vendor tree cannot prove the final
workspace feature union.
