# Functional Modules

## Module Rules

Product modules own user-visible responsibilities and durable workflows. They
are neither navigation labels nor code packages. A module can span several
desktop surfaces and application services, but its authority and outcomes must
remain unambiguous.

## Module Map

| ID | Module | Owns |
| --- | --- | --- |
| M01 | Desktop Shell And Workspace | Resident desktop presence, workbench, navigation, and activity entrypoints. |
| M02 | Connection Center | Connection catalogues, profiles, projects, routing, and lifecycle. |
| M03 | Trust And Credentials | Host identity, credential references, and trusted authentication decisions. |
| M04 | Server Workspace | Server-oriented context, state, and Human workspace composition. |
| M05 | Terminal Workspace | Human SSH/PTY sessions, terminal interaction, and session-aware lifecycle. |
| M06 | Files And Transfers | SFTP browsing, transfers, previews, artifacts, and bounded editing. |
| M07 | Tunnels And Network Diagnostics | Forwarding, route visibility, and bounded network diagnostics. |
| M08 | Server Insights | Resource, process, service-health, and diagnostic summaries. |
| M09 | Logs Workspace | Scoped log discovery, querying, tailing, and redacted sharing. |
| M10 | Service Operations | Service inspection and controlled lifecycle operations. |
| M11 | Controlled Changes | Typed change execution, confirmation, evidence, and recovery semantics. |
| M12 | Agent Governance | Agent Scope, template, Session, revocation, and global enablement management. |
| M13 | Agent Capability Operations | Typed Agent requests, capability execution, and bounded result delivery. |
| M14 | Agent Trail | Protected, Human-readable operation and authorization history. |
| M15 | Settings, Artifacts, And Portability | Preferences, resource roots, exports, backups, and local portability. |

## Cross-Module Workflows

### Human Server Operation

The Human selects a Project and Connection, makes host-trust and credential
decisions, then works through a server workspace, terminal, files, services, and
diagnostics without granting any authority to an Agent.

### Agent Diagnosis

The Human configures an Agent Scope, starts a bounded Session, and the Agent
uses only registered Direct capabilities. Each result is redacted, correlated,
and recorded in Agent Trail.

### Agent Controlled Change

The same governance boundary applies to change operations. A typed Direct
execution contract, or a future normalized Plan, must be authorized and remotely
enforced; UI confirmation alone is never sufficient.

### Global Disablement

The Human can close global Agent admission without affecting Human operations.
Outstanding Agent authority settles according to its execution contract and is
recorded as protected audit evidence.
