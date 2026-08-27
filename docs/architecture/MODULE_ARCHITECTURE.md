# Module Architecture

## Layer Model

VigilShell has three directional layers:

1. Renderer features and product modules own presentation and Human interaction.
2. Rust application services orchestrate typed use cases and application ports.
3. Trusted domain cores own authority, policy, connection, persistence, and
   security invariants.

Dependencies point inward. Composition roots assemble concrete adapters, but
domain contracts do not import renderer, Tauri, or global-runtime concerns.

## Repository Map

| Path | Primary responsibility |
| --- | --- |
| `src/` | Svelte renderer features, project-owned UI wrappers, and typed desktop adapters. |
| `src-tauri/` | Tauri composition root, immutable desktop data layout, IPC mapping, native runtime ownership, permissions, and desktop lifecycle. |
| `crates/vigilshell-core/` | Runtime-independent domain values, application services, and inward-facing Ports. |
| `crates/vigilshell-storage-sqlite/` | SQLite persistence, migrations, transactions, and storage Port implementations. |
| `crates/vigilshell-credentials/` | Platform credential adapter: macOS secure input and Keychain, plus Windows native input and Current-User DPAPI. |
| `crates/vigilshell-lifecycle-macos/` | macOS native termination and lifecycle adapter. |
| `crates/vigilshell-transport-russh/` | Project-owned SSH transport and Human terminal adapter. |
| `vendor/russh/` | Pinned upstream source plus the reviewed VigilShell security hardening patch. |

## Initial Rust Backend Composition

The backend begins as a Rust modular monolith. The initial `vigilshell-core`
library owns domain values, application services, and the Ports required by
those services. It does not depend on Tauri, an async runtime, SQLite, a platform
credential provider, or an SSH implementation.

Storage, credential, transport, desktop, and protocol adapters depend inward on
Core-owned contracts. A concrete adapter maps its third-party failures before
they cross a Port. Crates are split only when implemented behavior, independent
lifecycle, or dependency isolation requires a separate owner; planned module
names alone do not justify placeholder crates.

## Service Families

- Desktop and workspace services manage workbench, navigation, lifecycle, and
  safe state projection.
- Connection and trust services manage profiles, routes, host trust, and
  credential references.
- Project and Managed Server services own Human navigation grouping, stable Server
  identity, Server naming, and the Server-to-Connection relationship. They do not
  own route trust, credentials, or transport authority.
- Human operation services manage terminals, files, transfers, tunnels, and
  server operations.
- Agent governance services manage Scopes, Sessions, templates, revocation, and
  policy input.
- Agent invocation services normalize and dispatch typed capabilities.
- Query and local-data services expose bounded read models, artifacts, and audit.

## Ownership Rules

Each resource has one authoritative owner. Human connection leases and Agent
transport leases remain distinct. Data is persisted or cached by the owner whose
invariants it represents; other layers consume typed ports rather than shared
mutable models. Composition remains explicit, including partial-startup unwind
and shutdown.

Project membership is an optional property of a Managed Server. Connection
Profiles inherit that navigation projection but retain independent route,
credential, Host Trust, security revision, and terminal lifecycle ownership.
Every remote Human operation continues to resolve and authorize an exact
Connection Profile; neither Project ID nor Managed Server ID is a transport target.

## Human Terminal Admission Ports

The first Human backend path uses four Core-owned Ports:

| Port | Owns at the boundary |
| --- | --- |
| Connection Profile Repository | Loads the exact saved profile, stable server identity, route, credential reference, enabled state, and security revision. |
| Host Trust Repository | Returns trust evidence bound to the same profile and security revision. |
| Credential Broker | Issues a short-lived opaque lease for the Human Terminal purpose after prior gates pass. |
| Human Terminal Transport | Opens a Human-only terminal from a Core-authorized request and validates the expected host identity during the real handshake. |

The caller supplies only a Connection Profile identifier and validated terminal
dimensions. It cannot supply an endpoint, username, credential snapshot, trust
decision, or transport object. The successful application result contains only
opaque session, server, and profile identifiers suitable for later projection.

## Operation Flows

Human operations resolve a Connection, verify trust, use an appropriate
credential reference, obtain a Human lease, and project safe state to the
renderer. Agent operations normalize a capability request and pass through the
Agent control plane before a purpose-bound execution lease can exist. Partial
results and failures retain typed ownership and never become generic strings.

For Human Terminal admission, Core validates the request before any Port call,
loads and checks the Connection, requires matching trust evidence, then asks the
Credential Broker for a Human-purpose lease, and only then calls the Human
Transport Port. A failure at any step prevents every later step. Port acceptance
does not prove that a remote terminal was opened; only the transport result may
create an opaque Human Terminal Session identifier.
