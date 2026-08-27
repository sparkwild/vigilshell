# Technology Baseline

## Accepted Application Stack

VigilShell uses Tauri 2 with Rust, Svelte 5, TypeScript, Vite, and xterm.js.
Rust owns trusted local behavior; Svelte renders safe view models through typed
application ports. The renderer is a plain Svelte SPA: SvelteKit and a
third-party router are not part of the initial baseline.

Tokio is the sole asynchronous runtime family. The desktop reuses Tauri's
runtime; standalone companions create one process-local runtime at their
composition root. Domain code does not own Tauri handles or runtime setup.

## Trusted Integration Choices

- `russh` with `russh-sftp` is the initial SSH/SFTP engine behind project-owned
  ports; there is no dual backend or system-OpenSSH fallback.
- The official Rust `rmcp` crate is the initial stdio-only MCP implementation
  surface. It remains an adapter and never an authority boundary.
- SQLite is the private durable-state store. Platform credential providers hold
  secrets; SQLite and the renderer retain opaque references only.
- `ts-rs` 12.0.1, pinned exactly under its MIT license, generates declarations
  only for explicit renderer-safe Rust DTOs. Its serde-compatible output is kept
  deterministic and checked for drift. Tauri adapters retain command, permission,
  error-mapping, and runtime ownership; generated declarations do not expose
  domain records, secrets, native handles, or unrestricted invocation primitives.

The SQLite adapter uses bundled SQLite through `rusqlite`, one process-local
Mutex-protected connection, WAL, FULL synchronous durability, foreign keys,
untrusted-schema protection, cell-size checking, and a bounded busy timeout.
An application ID and storage-generation record distinguish the current Fresh
Schema from the accepted one-time legacy v1-v3 inputs.

The macOS Human credential adapter uses Security.framework directly
through `security-framework` 3.7 with its `OSX_10_15` feature. It stores SSH
passwords as non-synchronizing generic-password items in the data-protection
Keychain. The app identifier-derived service is
`com.sparkwild.vigilshell.credentials`, and the Keychain account is the opaque
`CredentialRef`; remote hosts and usernames are not Keychain lookup keys.
The macOS deployment target is therefore 10.15 or later.

Password capture uses AppKit `NSSecureTextField` through the safe generated APIs
in `objc2` 0.6 and `objc2-app-kit`/`objc2-foundation` 0.3. This platform adapter
runs only on the macOS main thread and returns no secret value to Tauri IPC.
Secret buffers use `zeroize` and are consumed by the trusted transport boundary.
The Windows adapter uses `windows-sys` bindings for the native password-only
credential prompt and Current-User DPAPI. It forbids DPAPI UI and machine-wide
protection, stores only versioned ciphertext envelopes under the verified
private-data root, and binds each file to the digest of the existing opaque
`CredentialRef`. Neither platform uses a renderer password field, cloud
synchronization, or a plaintext file fallback.

Trusted native identifiers for new Managed Servers, Connection Profiles, and
Credential references use UUID v4 from `uuid` 1.24 with only the `v4` feature.
These identifiers are correlation identities rather than authorization tokens;
all operations still revalidate the referenced record and its expected version.

The initial Human SSH client pins `russh` 0.62.6 with default features disabled
and only the `ring` and `rsa` features enabled. The repository carries a narrow,
same-version vendored hardening patch because the upstream release retains a
password `String` after successful authentication and includes protocol-payload
debug paths. The patch uses zeroizing, redacted password storage, clears auth
state on every settlement, zeroizes plaintext authentication staging buffers,
and removes production payload-byte logging. Cargo must resolve the patched path;
falling back to the registry release is a security failure.

The client allows Ed25519, ECDSA P-256/P-384/P-521, and RSA keys negotiated with
RSA-SHA2-512 or RSA-SHA2-256. DSA, SHA-1 `ssh-rsa` signatures, certificate Host
Keys, security-key Host Keys, compression, DES, and algorithm downgrade are not
part of the initial path. Probe and authenticated connections use the same
policy. Tokio remains the only async runtime, with Tauri IPC `Channel` carrying
ordered raw terminal `ArrayBuffer` payloads and typed lifecycle events to the
originating WebView.

The initial key-exchange allowlist is Curve25519 SHA-256 under the standard and
libssh compatibility names, Diffie-Hellman group exchange SHA-256, group14
SHA-256, and groups15 through 18 with SHA-512. SHA-1 key exchange and ML-KEM are
not in the initial vetted offline dependency set. The fork removes unused
AWS-LC, ML-KEM, DSA, DES, compression, and benchmark/example-only dependency
surfaces and records each delta; restoring one requires a new compatibility and
supply-chain review rather than a runtime downgrade switch.

## Renderer And Desktop Choices

Bits UI and shadcn-svelte are the standard control baseline, consumed through
project-owned wrappers and tokens. Tauri core `tray-icon`, official
single-instance handling, and official autostart are used only through Rust
adapters; the initial renderer does not receive their JavaScript guest APIs.

The macOS notification route is a project-owned `NotificationPort` backed by
`UNUserNotificationCenter`, allowing a zero-WebView action to return an opaque
reference to Rust for revalidation.

## Compatibility Discipline

Unless an integration is explicitly frozen above, concrete versions, features,
cryptographic backends, and package installation are not implied by this
baseline. Before each technology is frozen into an implementation, a focused
compatibility check must prove the required behavior, license/supply-chain
posture, and failure semantics through the project-owned boundary.
