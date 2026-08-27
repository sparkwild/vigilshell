# VigilShell Codex Instructions

This file is the compact repository entrypoint for coding Agents. Durable
project facts and standards live under [`docs/`](docs/INDEX.md); select only the
documents relevant to the current task.

## Task Routing

Before changing documentation, read [`docs/INDEX.md`](docs/INDEX.md),
[`docs/development/DOCUMENTATION_STANDARDS.md`](docs/development/DOCUMENTATION_STANDARDS.md),
and the index for each category being changed.

For other work, start with the narrowest matching route:

| Work | Read |
| --- | --- |
| Product purpose, terminology, or scope | [`docs/overview/INDEX.md`](docs/overview/INDEX.md) |
| Feature behavior, workflows, UI, or Agent capabilities | [`docs/features/INDEX.md`](docs/features/INDEX.md), then the named feature owner |
| Technology or dependency selection | [`docs/technology/INDEX.md`](docs/technology/INDEX.md) |
| Topology, trust boundaries, storage, transport, or MCP | [`docs/architecture/INDEX.md`](docs/architecture/INDEX.md), then the matching architecture owner |
| Implementation, tests, review, build, or release | [`docs/development/INDEX.md`](docs/development/INDEX.md) |

For SSH, SFTP, Host Trust, credentials, terminal runtime, proxies, forwarding,
or connection pooling, read
[`TRANSPORT_AND_TERMINAL.md`](docs/architecture/TRANSPORT_AND_TERMINAL.md).
For persistence, migrations, credentials, resource roots, backup, or recovery,
read [`LOCAL_PERSISTENCE.md`](docs/architecture/LOCAL_PERSISTENCE.md). For MCP or
Agent enforcement, read
[`AGENT_CONTROL_PLANE.md`](docs/architecture/AGENT_CONTROL_PLANE.md) and
[`AGENT_CAPABILITIES.md`](docs/features/AGENT_CAPABILITIES.md).

Current plans, progress, review evidence, and handoffs belong in the active
work-management location, not in durable project documents.

## Repository Boundary

- This repository is the independent `sparkwild/vigilshell` codebase. The former
  Bifrost workspace is historical context only; never copy, move, restore, or
  import files from it.
- Inspect the live repository before relying on historical context. Do not create
  speculative scaffolding to make planned architecture appear implemented.
- Freeze accepted product, architecture, and security decisions in their owning
  documents before broad implementation, and stop at the requested phase boundary.
- Preserve unrelated working-tree changes. Do not perform Git or GitHub mutations
  unless the user explicitly requests them.

## Global Trust Boundaries

- Rust owns connection establishment, Host Key verification, credential access,
  policy, privileged execution, redaction, and audit. The renderer receives safe
  view models and opaque identifiers only.
- Human Lane and Agent Lane use separate entrypoints, sessions, capabilities, and
  authorization paths. Shared low-level code must not collapse that boundary.
- MCP is an adapter into the Agent Lane, never an SSH or credential authority.
  Every Agent operation remains subject to Scope, policy, risk, execution guards,
  redaction, and audit.
- Frontend features call typed application ports. Direct Tauri invocation stays
  inside designated desktop adapters.
- Never place real credentials, hosts, private keys, tokens, or sensitive server
  output in source, fixtures, logs, screenshots, or test artifacts.

## UI And Verification

- The running Tauri application is the product UI source of truth. Browser-only
  shells and screenshots cannot prove native lifecycle, secure IPC, credentials,
  terminal behavior, or window integration.
- Use maintained open-source components for standard controls through
  project-owned wrappers and tokens. Custom controls need a concrete product
  reason and focused validation in the real application.
- Before editing, state the planned files and narrow validation commands. Build
  one reviewable vertical slice, run the smallest meaningful checks, then expand
  according to risk.
- Before completion, confirm ownership and links, report commands actually run,
  preserve known gaps, and distinguish documented intent from implemented and
  release-qualified behavior.
