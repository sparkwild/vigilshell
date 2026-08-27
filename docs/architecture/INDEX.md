# Architecture Documentation

This category owns VigilShell's system topology, component boundaries, runtime
behavior, data ownership, and trust boundaries.

## Belongs Here

- System structure, Human/Agent separation, ports, lifecycle, storage, transport, and trusted enforcement.

## Does Not Belong Here

- User feature catalogues, package-selection rationale, implementation status, or contributor workflow rules.

## Current Documents

- [SYSTEM_ARCHITECTURE.md](SYSTEM_ARCHITECTURE.md) — resident desktop topology, local IPC, and lifecycle.
- [MODULE_ARCHITECTURE.md](MODULE_ARCHITECTURE.md) — repository map, layer ownership, service families, and dependency direction.
- [AGENT_CONTROL_PLANE.md](AGENT_CONTROL_PLANE.md) — MCP containment, Heimdall enforcement, and Agent execution flow.
- [TRANSPORT_AND_TERMINAL.md](TRANSPORT_AND_TERMINAL.md) — SSH/SFTP, PTY, renderer, and connection boundaries.
- [LOCAL_PERSISTENCE.md](LOCAL_PERSISTENCE.md) — private storage, credentials, durability, and recovery boundaries.
