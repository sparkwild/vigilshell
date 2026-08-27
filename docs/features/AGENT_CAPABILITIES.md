# Agent Capabilities

## Catalog Contract

A Capability is a stable, versioned business intent rather than a raw transport
primitive. Catalog registration, Scope grant, Session availability, policy,
execution guarding, redaction, and audit are all required before an Agent can
use one. Documentation of an entry is not runtime availability.

Each request uses one canonical capability with allowlisted `include` options and
bounded limits. A capability may fan out internally under one Operation ID and
return labeled partial results, continuation data, and machine-readable recovery
without exposing unavailable resources or authority.

## Tool Shape And Concurrency

Tools expose stable task-level business intents with the fewest practical round
trips. Do not create overlapping atomic, aggregate, and shortcut variants, an
unbounded catch-all tool, or an arbitrary item limit that does not correspond to
authority, destructive risk, correctness, or measured capacity. An authorized
workflow must not require a Human to relay machine values or discard useful safe
success data merely to reduce the apparent tool surface.

The Agent Lane accepts independent Direct invocations concurrently. Internal
reads may fan out concurrently under one Operation ID, but the App does not add a
global serial queue or implicit batch. Actual resource conflicts, capacity, and
security invariants remain enforceable per invocation.

## Stable Capability Families

| Capability | Intent |
| --- | --- |
| `agent.session.context@1` | Return the Session's safe context and effective limits. |
| `server.inspect@1` | Inspect a granted managed server. |
| `service.inspect@1` | Inspect scoped service state. |
| `logs.query@1` | Query bounded, redacted log data. |
| `file.list@1` | List resources within an explicit file grant. |
| `file.get@1` | Deliver bounded inline content or a governed download artifact. |
| `network.probe@1` | Run bounded network diagnostics. |
| `service.control@1` | Perform a typed, policy-governed service operation. |
| `config.patch@1` | Apply a typed, guarded configuration change. |
| `server.reboot@1` | Request a tightly governed server restart. |

## Access Resolution And Scope

Diagnostic Access, Routine Operations Access, and Full Access are versioned
templates that resolve to explicit Capability sets. Custom Access stores its own
explicit set and constraints. Editing a template-defined Capability or constraint
saves the configuration as Custom Access without source-template metadata;
reselecting a template is the reset mechanism.

Resource-range selection is an independent Scope dimension. A new Scope may show
Diagnostic Access as draft UI state, but no authority exists until the trusted
Core commits a valid Scope configuration. Full Access is never a wildcard and
does not absorb newly introduced Capabilities.

## Governed File Access

Diagnostic file access is limited to explicit file or directory grants projected
through safe aliases and enforced by the Rust File Guard. A directory grant is a
recursive canonical-root boundary over current and future descendants. It does
not authorize a parent, sibling, symbolic-link escape, write-like operation, or
ungranted discovery. The initial model has no directory exclusions, glob filters,
or sensitive-name blacklist; narrower roots and explicit file grants define the
boundary.

`file.get@1` uses `auto`, `inline`, or `download` delivery, with `auto` as the
default. Inline content is bounded and redacted. Explicit `inline` may return
`download_required` without creating an artifact; explicit `download` never
bypasses Scope or transfer limits. A download preserves original remote bytes
only in the App-created resource directory or a Human-configured resource root;
an Agent cannot select a destination, and that directory has no inferred Agent
workspace meaning. Atomic success returns the ArtifactId and exact final path
without duplicating original bytes in the response. Path disclosure grants no
authority over unrelated local files. File type, encoding, size, and transfer
thresholds are versioned Rust policy and are not request, IPC, Scope, or Settings
overrides.

Resource-root ownership and relocation rules live in
[LOCAL_PERSISTENCE.md](../architecture/LOCAL_PERSISTENCE.md).

## Execution And Confirmation

Production writes require a typed Direct execution contract or, for a future
composite capability, a normalized Operation Plan plus remote enforcement.
Plan-required work uses a native ConfirmationGrant, and changing the normalized
Plan invalidates that grant. Frontend confirmation alone is never an authority.

The full enforcement sequence is owned by
[AGENT_CONTROL_PLANE.md](../architecture/AGENT_CONTROL_PLANE.md).

## Risk And Availability

Capabilities carry a risk tier and default state. High-risk entries are disabled
until the Human sees a consequence warning and completes a second confirmation
bound to the exact Scope configuration. Initial release exposure may be read-only,
but the authorization architecture is not read-only-only: later controlled writes
use the same catalog, Scope, Session, confirmation, dispatch, redaction, and audit
path.

A confirmed high-risk Direct invocation does not prompt again merely because of
its risk tier; it still passes the normal per-invocation enforcement path.

Prohibited entries include raw shell execution, arbitrary URL access, arbitrary
file discovery, raw SSH/SFTP handles, real endpoint disclosure, and credential
access. Full Access remains an explicit versioned set; it never becomes a
wildcard that absorbs newly introduced capabilities.

## Result Semantics

Results distinguish complete, partial, denied, unavailable, cancelled, and
outcome-unknown states with stable machine-readable recovery details. An effectful
operation whose remote outcome is uncertain is never silently replayed.
