# System Architecture

## Runtime Topology

VigilShell is a per-user resident desktop system. Its trusted Rust Core remains
available while the on-demand Svelte renderer is closed. The Core owns connection
establishment, local state, Agent admission, policy enforcement, operation
settlement, redaction, audit, notifications, and native lifecycle integration.

The system has four runtime participants:

- the resident desktop executable and composition root;
- one or more on-demand renderer windows with safe view models only;
- the `vigilshell-mcp` stdio companion as a local Agent-facing bridge; and
- remote executors reached only through Core-owned, purpose-bound paths.

## Local IPC And Renderer Boundary

Frontend features call typed application ports. Direct Tauri invocation is
confined to designated adapters. IPC accepts and returns renderer-safe DTOs,
opaque identifiers, bounded diagnostics, and explicit error semantics; it does
not carry credentials, raw SSH handles, unrestricted execution primitives, or
unredacted protected output.

## Resident Lifecycle

Process start initializes trusted storage, policy, native lifecycle adapters,
and local admission before any renderer is considered authoritative. Closing the
last normal workbench window hides and retains that WebView and does not stop the
resident Core. Explicit Quit atomically closes admission before new effectful
operations, settles already admitted work and native terminal ownership within
one bounded shutdown deadline, flushes required durable state, and exits in
defined order. Repeated Quit requests join the same settlement rather than
starting competing shutdowns.

Single-instance activation, login start, the menu-bar or tray controller, and
native notifications are Core-owned adapters. Their visible state must reflect
trusted lifecycle state rather than renderer inference.

## Cross-Lane Isolation

The Human Lane and Agent Lane use different entrypoints, sessions, capability
models, and authorization paths. Shared low-level libraries may share transport
mechanics but must never turn a Human terminal, credential use, or connection
lease into Agent authority. An Agent-only transport can be reused only within the
same authorized Agent Session and still requires fresh authorization for each
invocation.

## Diagnostics And Audit

Diagnostics support local recovery; protected audit supports accountability.
They are separate read models with bounded correlation identifiers. Redaction
happens before rendering, persistence, telemetry, or inline MCP delivery.
