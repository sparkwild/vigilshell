# VigilShell Codex Instructions

This file is the repository entrypoint for agent behavior. It routes work to the
durable standards owned by `docs/`; it does not replace those standards.

## Mandatory Routing Reads

Before changing documentation, read:

1. `docs/INDEX.md`
2. The `INDEX.md` in every documentation area touched by the change

Before changing product behavior, architecture, security boundaries, UI, tests,
or implementation code, also read:

1. `docs/overview/INDEX.md`
2. `docs/features/INDEX.md`
3. `docs/technology/INDEX.md`
4. `docs/architecture/INDEX.md`
5. `docs/development/INDEX.md`

For current task state, implementation progress, review evidence, or handoff
material, inspect the active work-management location rather than treating a
durable document as a status source.

For changes to product-module ownership, Agent Capability flow, or Heimdall
responsibilities, also read the applicable dedicated documents:

- `docs/features/FUNCTIONAL_MODULES.md`
- `docs/features/AGENT_CAPABILITIES.md`
- `docs/architecture/MODULE_ARCHITECTURE.md`
- `docs/architecture/AGENT_CONTROL_PLANE.md`

For changes to SSH, SFTP, host trust, authentication, proxies, jump hosts,
forwarding, connection pooling, or transfer-engine behavior, also read:

- `docs/architecture/TRANSPORT_AND_TERMINAL.md`

For changes to local persistence, schema migration, backup or restore behavior,
private storage routing, or credential-store adapters, also read:

- `docs/architecture/LOCAL_PERSISTENCE.md`

For changes to the MCP SDK, stdio bridge, protocol negotiation, dynamic tool
projection, MCP results, cancellation, progress, or companion packaging, also
read:

- `docs/architecture/AGENT_CONTROL_PLANE.md`

Task-lifecycle material never overrides a durable document. Extract an accepted
project rule into its owning documentation category instead of preserving two
sources of truth.

## Documentation Destinations

- `docs/overview/`: project purpose, scope, terminology, and non-goals
- `docs/features/`: capabilities, workflows, behavior, and feature boundaries
- `docs/technology/`: accepted selections, rationale, and compatibility constraints
- `docs/architecture/`: system topology, ownership, dependency direction, and
  application boundaries
- `docs/development/`: coding, testing, build, release, review, and
  documentation-maintenance standards

Keep a durable rule in the area that owns it. When a document moves, update this
file and all affected indexes in the same change.

## Repository Boundary

- This repository is the independent `sparkwild/vigilshell` codebase.
- The former Bifrost working directory is historical context only. Do not copy,
  move, restore, or import files from it.
- Inspect the live repository before relying on historical task context.
- Do not create speculative scaffolding merely to make a planned architecture
  look implemented.

## Product Context

- VigilShell is an Agent-Native professional server-management desktop app.
- Human users receive a complete SSH, terminal, SFTP, monitoring, and server
  operations experience through the Human Lane.
- Agents operate only through typed, scoped capabilities in the Agent Lane.
- Agents must not receive raw SSH access, real server addresses, credentials,
  arbitrary shell execution, arbitrary file access, or arbitrary URL access.
- Databases, caches, queues, and similar systems are managed as services on a
  server. VigilShell is not a separate database IDE.
- The baseline stack is Tauri 2, Rust, Svelte 5, TypeScript, Vite, and xterm.js.

## Execution Discipline

- Freeze product, architecture, and security decisions in the owning docs before
  broad implementation.
- Before editing, state the planned files and the narrow validation commands.
- Prefer one reviewable vertical slice over a wide collection of placeholders.
- Inspect existing dependencies and conventions before adding packages.
- Preserve the repository package manager and lockfile once they exist.
- Stop at the requested phase boundary; documentation work is not permission to
  scaffold the application.
- Do not perform Git or GitHub mutations unless the user explicitly requests them.

## Architecture Boundaries

- Rust owns connection establishment, host-key verification, credential access,
  policy decisions, local Human-confirmation grants, privileged execution, output
  redaction, and audit.
- The renderer receives safe view models and opaque identifiers, never secrets,
  raw SSH handles, or unrestricted execution primitives.
- Human Lane and Agent Lane must use distinct entrypoints, sessions, capability
  models, and authorization paths. Shared low-level libraries must not collapse
  their trust boundaries.
- MCP is an adapter into the Agent Lane. It must not connect to SSH, read the
  credential store, or bypass Heimdall Core.
- Every Agent operation must pass scope, policy, risk, execution guard, redaction,
  and audit controls before it is considered complete. Plan-required operations
  additionally require a local Human confirmation; Direct operations within
  configured bounds do not prompt the Human per call.
- The Agent Lane accepts multiple independent Direct invocations in flight. Do
  not impose global serialization, an implicit App-managed batch, or a hidden job
  queue; enforce only actual resource conflicts, capacity, and security invariants
  per invocation.
- Design MCP tools as stable task-level capabilities with minimal practical round
  trips and directly consumable typed results. Do not fragment an authorized
  workflow, require Human value transfer, hide useful success data, or impose an
  arbitrary item-count limit merely to appear safer. Every restriction must map
  to a concrete authority boundary, destructive risk, correctness invariant, or
  measured capacity limit.
- Use one canonical tool per business intent with capability-owned, allowlisted
  `include` options and bounded limit fields. Do not create overlapping atomic,
  aggregate, and shortcut variants or an unbounded catch-all tool. Internal reads
  may fan out and run concurrently under one Operation ID; return labeled partial
  results, bounded continuation, and stable machine-readable recovery without
  leaking unavailable authority or resources.
- Design the versioned Capability Catalog, risk tiers, default states,
  availability, Scope grants, and confirmation policy as one durable path from
  the initial release. Initial read-only exposure is a release choice, not
  permission to create a temporary read-only authorization architecture.
- Treat `docs/features/AGENT_CAPABILITIES.md` as the owner of stable Agent
  Capability identities, common MCP request/result contracts, template-resolution
  intent, and runtime registration gates. A documented entry is not available
  until its production handler and every gate in that document are complete.
- Access choices are the Diagnostic Access, Routine Operations Access, and Full
  Access templates plus editable Custom Access. Persist explicit resolved
  Capability sets and versions, not dynamic roles. Editing a template's
  Capabilities or template-defined constraints saves the selection as Custom
  Access with no source-template metadata; resource-range selection remains an
  independent Scope dimension. Scope configuration remains editable, and
  reselecting a template is the reset mechanism.
- New Agent Scope UI preselects Diagnostic Access as draft state only. Do not
  treat that default as authority before the trusted core commits a valid Scope
  configuration.
- Diagnostic Access may read only explicitly granted file or directory resources
  through safe aliases and the Rust File Guard. Reject ungranted discovery, path
  traversal, symbolic-link escape, write-like operations, and unredacted inline
  MCP content.
- Directory grants are recursive canonical-root boundaries and include current
  and future descendants. They never authorize parent, sibling, or out-of-root
  symbolic-link targets, and the UI must disclose the recursive effect.
- Do not add initial directory exclusions, glob filters, or sensitive-name
  blacklists. Use narrower roots or explicit file grants; future exclusion
  semantics require separate review and must not reinterpret existing grants.
- File-read delivery distinguishes bounded redacted inline content from an
  original-byte SFTP artifact. Original artifacts may be written only to the
  App-created `${user_home}/VigilShell` resource directory or a Human-configured custom
  directory; Agent calls cannot choose local destinations, the directory has no
  inferred Agent-workspace semantics, and no response duplicates original bytes.
  After atomic success, MCP must return the `ArtifactId` and exact final absolute
  path so the authorized Agent can continue without a Human handoff. Do not treat
  path disclosure as authority to choose destinations or access unrelated files.
- Initial file type, encoding, size, and transfer thresholds are versioned built-
  in `FileReadPolicy` defaults owned by Rust. Do not expose Settings, Agent Scope,
  IPC, or MCP inputs that can override them until a separate product and security
  review approves such configuration.
- Agent file retrieval is one typed capability with `auto`, `inline`, and
  `download` delivery modes; `auto` is the default. It returns eligible bounded
  inline content or completes an original-file download and returns its path in
  the same invocation. Explicit `inline` may return `download_required` without a
  local artifact, and explicit `download` never bypasses Scope or transfer limits.
- Resolve `user_home` through the native platform adapter. Never expand a literal
  `~` in domain code or fall back to the working directory, temporary directory,
  or a broader path when the default resource root cannot be created.
- The user resource root owns `downloads/`, `exports/`, and `backups/` only.
  Databases, Scope/Session and audit state, journals, caches, and credentials stay
  in platform-owned private storage. A custom resource root must not relocate or
  expose those private stores.
- Resource-root changes affect future artifacts only. Do not move, copy, delete,
  scan for, or heuristically relink existing artifacts; retain their committed
  locations and report missing files explicitly.
- High-risk capabilities default off. Enabling or expanding one requires a
  consequence warning and second confirmation bound to the exact Agent Scope
  configuration. A confirmed high-risk Direct invocation does not prompt again
  merely because of its risk tier.
- Production Agent writes require a typed Direct execution contract or, for a
  future composite capability, a normalized Operation Plan, plus remote
  enforcement. Frontend confirmation alone is never a security control.
- Frontend features call typed application ports. Direct Tauri invocation is
  confined to designated adapter modules.

## UI Source Of Truth

- The running Tauri desktop application is the sole product UI source of truth.
- Do not introduce Storybook as an initial UI development or acceptance layer.
- Do not maintain a parallel browser-only product shell or throwaway component
  implementation.
- Prefer a stable open-source component library for standard controls. Custom
  buttons, menus, dialogs, forms, focus management, and overlay behavior require
  a documented reason when a maintained library component can satisfy the need.
- Feature code consumes project-owned component wrappers and design tokens rather
  than styling third-party internals ad hoc.
- Terminal, file-transfer, topology, and other domain-specialized surfaces may
  require custom composition, but their standard controls still follow the shared
  component contract.
- UI fixtures must run through a debug/test-only adapter inside the real Tauri app
  and must not ship in production builds.

## Security And Data Handling

- Never place real credentials, host addresses, private keys, tokens, or sensitive
  server output in source, fixtures, logs, screenshots, or test artifacts.
- Secrets stay in the trusted native boundary and are referenced by opaque IDs.
- A Plan-required `ConfirmationGrant` binds the normalized plan, target scope,
  Plan Hash, Human actor, expiry, and single-use dispatch context. A changed plan
  invalidates the confirmation grant.
- Security-sensitive failures fail closed and remain auditable.
- Redaction happens before untrusted rendering, audit persistence, inline MCP
  content, or telemetry. An explicitly authorized original-file transfer artifact
  may preserve remote bytes only in the configured download destination; all
  surrounding metadata remains bounded and redacted.

## Verification

- Use the narrowest meaningful check while iterating, then expand in proportion to
  the risk of the change.
- Once application tooling exists, validate pure logic with unit tests, native
  behavior in the real Tauri app, and critical workflows with targeted desktop
  end-to-end tests.
- Browser previews, screenshots, and mocked frontends cannot by themselves prove
  native behavior, secure IPC, secret handling, or window integration.
- Report commands actually run, results, and any checks not run.

## Completion Expectations

Before declaring work complete:

1. Confirm the change is in the correct ownership area.
2. Confirm relevant indexes and links remain accurate.
3. Run the scoped guardrails and tests that exist.
4. Report changed files, verified behavior, known gaps, and the next safe step.
