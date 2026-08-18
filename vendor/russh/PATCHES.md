# VigilShell Fork Patches

This fork is intentionally narrow: it hardens the reachable client password
path and production logging while reducing dependency and algorithm surfaces to
the product baseline. It is not a general upstream replacement for every
optional `russh` capability.

## Secret Lifetime and Debug Output

- Passwords and client authentication usernames use zeroizing storage.
- `auth::Method`, client authentication messages, and `CommonSession` have
  explicit `Debug` implementations that reveal only variants, state, counts, or
  lengths. Password values are always rendered as `<redacted>`.
- Password and username state is cleared immediately after a password
  `USERAUTH_REQUEST` is encoded. Success, failure, encoding error, session exit,
  and `Drop` paths also clear it idempotently.
- Password packet staging reserves the exact remaining packet size before the
  secret is written, avoiding reallocation after password bytes enter that
  buffer.
- The reusable plaintext packet staging allocation is zeroized across its full
  initialized and spare capacity after every flush. The packet-encryption error
  path wipes and resets the cursor before returning.

Dropping only an `authenticate_password` future after its message has been sent
does not cancel the owning session and is not a supported cancellation boundary.
The application transport must send a best-effort disconnect and drop the
entire `Handle` for cancellation or timeout. Dropping the handle closes the
message sender; queued authentication messages and the session-owned method are
then cleared by their zeroizing wrappers and session shutdown path. A handle
with an abandoned password future must never be reused.

## Production Log Redaction

Production logging was audited across the crate. Authentication packets,
plaintext protocol buffers, terminal and channel payloads, banners, agent
messages, keys and signatures, arbitrary peer strings, handler errors, and
derived structures containing queued bytes are no longer formatted into logs.
Logs retain only fixed event names and bounded metadata such as variants, IDs,
booleans, lengths, and counts.

## Dependency and Algorithm Surface

- Default features are `ring` and `rsa`; VigilShell consumers still select
  `default-features = false` with explicit `ring,rsa`.
- Removed feature and dependency surfaces: AWS-LC, ML-KEM hybrid KEX, DSA,
  DES/3DES, flate2 compression, the legacy Ed25519 PKCS#8 parser, and benchmark-
  or example-only dependencies and targets that are not part of the product.
- `ssh-key` 0.7.0-rc.11 is nested and patched to remove its DSA feature and
  dependency definition while preserving the algorithms needed by this crate.
- The `ssh-key` dependency disables defaults and enables only `std`, Ed25519,
  P-256, P-384, P-521, SHA-1 parsing support, and optional RSA selected by the
  existing `russh/rsa` feature. Its `encryption` and `ppk` features are not
  enabled, and its `ssh-cipher` dependency also disables defaults.
- Encrypted OpenSSH private keys and PuTTY PPK files are rejected by this fork.
  Unencrypted Ed25519, ECDSA, and RSA key parsing and public-key authentication
  remain available. The separate existing PKCS#5/PKCS#8 parser is unchanged.
- The runtime adapter must still apply the product KEX allowlist. This crate
  surface reduction is not a substitute for that configuration.

Some upstream source files and fixtures for removed capabilities remain as
provenance material, but their modules or test gates are unreachable and no
Cargo feature can re-enable the removed surfaces.

Cargo's lockfile can still record the registry package `des` because the
transitive `ssh-cipher` manifest declares an optional `tdes` dependency. The
production `vigilshell-transport-russh` resolution has no dependency path to
it, and the production feature tree contains no `ssh-key/default`,
`ssh-key/encryption`, `ssh-key/ppk`, `ssh-cipher/default`, or
`ssh-cipher/tdes`. Removing an inert lockfile record would require a separate
`ssh-cipher` fork and is not evidence that DES is compiled.

## Verification Seams and Limits

Focused tests prove:

- password `Debug` redaction across method, queued message, and session state;
- immediate clearing on `USERAUTH_SUCCESS` and `USERAUTH_FAILURE`;
- clearing when a method, queued message, or session is dropped;
- clearing immediately after both initial and subsequent password request
  encoding paths; and
- invocation of the full-capacity staging-buffer wipe after flush.

The test-only observation flags prove that the relevant clear and wipe paths
were invoked. Safe Rust cannot inspect a `Vec` allocation's spare capacity after
it has been cleared, so the allocation-level assertion relies on `zeroize`'s
documented `Vec` implementation, which clears initialized elements, spare
capacity, and length. Zeroization is best effort: it cannot erase independent
copies retained by the caller, copies created outside this fork, or stale bytes
from allocations that existed before the exact-reserve rule. Server-side
password handling is outside the VigilShell reachable-client proof.

The full offline test suite is run for all targets. Strict Clippy is run for the
production library with warnings denied. Applying the project's production
pedantic lints to all upstream test sources reports roughly 295 existing
test-only findings (primarily deliberate unwrap/expect/indexing and async test
harness patterns); this fork does not suppress them with broad blanket allows.
