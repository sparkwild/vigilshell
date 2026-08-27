# VigilShell

VigilShell is a local-first professional server-management desktop application.
It gives Human operators a native SSH workbench while keeping future Agent
operations behind a separate, typed, explicitly authorized control plane.

The trusted Rust boundary owns connections, credentials, Host Trust, policy,
redaction, and privileged execution. The Svelte renderer receives only bounded
view models and opaque identifiers.

## Current Development Surface

The current macOS-first path includes:

- Project, Managed Server, and Connection Profile management;
- native password capture and Keychain-backed credential storage;
- explicit SSH Host Key probing and trust confirmation;
- Human terminal sessions rendered with xterm.js; and
- resident desktop lifecycle, menu-bar access, and single-instance activation.

The broader product scope includes files and transfers, tunnels, diagnostics,
server insights, logs, services, controlled changes, and a governed Agent Lane.
A documented capability is not considered available until its production
handler and security gates are complete.

## Technology

VigilShell uses Tauri 2, Rust, Svelte 5, TypeScript, Vite, xterm.js, SQLite,
and project-owned adapters around SSH, native credentials, and desktop
integration. Standard controls use Bits UI and shadcn-svelte through
project-owned wrappers.

See the [technology baseline](docs/technology/TECHNOLOGY_BASELINE.md) for the
accepted selections and security constraints.

## Run Locally

The current native credential and lifecycle path targets macOS 10.15 or later.
Install Rust and Node.js with Corepack, then run:

```sh
corepack enable
pnpm install
pnpm tauri dev
```

Common validation commands are documented in the
[engineering standards](docs/development/ENGINEERING_STANDARDS.md).

## Documentation

- [Project overview](docs/overview/PROJECT_OVERVIEW.md)
- [Product requirements](docs/features/PRODUCT_REQUIREMENTS.md)
- [Architecture index](docs/architecture/INDEX.md)
- [Agent capability contract](docs/features/AGENT_CAPABILITIES.md)
- [Development standards](docs/development/INDEX.md)
- [Complete documentation index](docs/INDEX.md)

## Security Model

Human and Agent operations use distinct entrypoints and authorization paths.
Agents never receive raw SSH access, credentials, arbitrary shell execution,
arbitrary file access, or unrestricted URLs. Detailed enforcement boundaries are
defined in the [Agent control plane](docs/architecture/AGENT_CONTROL_PLANE.md)
and [transport architecture](docs/architecture/TRANSPORT_AND_TERMINAL.md).
