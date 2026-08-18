# Agent Control Plane

## Boundary

MCP is an adapter into the Agent Lane, not an authority boundary. It does not
connect to SSH, access credential stores, or bypass Heimdall Core. The bridge
projects only capabilities that are registered, released, granted, available,
and executable under the current Scope and Session.

## Heimdall Core

Heimdall Core is the trusted enforcement path for Agent work. It resolves
authority, normalizes the invocation, evaluates policy and risk, verifies any
ConfirmationGrant, manages resource and capacity guards, issues dispatch
authority, supervises execution, redacts the result, and records protected audit
evidence.

Its contracts include an Agent request context, resolved authority, normalized
invocation, authorization decision, single-use dispatch ticket, execution
evidence, and protected capability result. A changed normalized Plan invalidates
its confirmation grant.

## Authorization And Enforcement

Every invocation passes Scope, policy, risk, execution-shape, target-specific
guards, redaction, and audit. Remote enforcement remains required for writes;
frontend confirmation is never security control. Revocation, Scope narrowing,
or global Agent disablement prevents a new business step and lets in-flight work
reach only its contract-declared safe cancellation or cleanup point.

## Agent-Facing Semantics

MCP negotiation, dynamic tool projection, cancellation, progress, correlation,
and result mapping are stable adapter concerns. Tool responses identify partial
data, bounded continuations, and safe recovery without leaking authority,
secrets, raw endpoints, or protected diagnostics.
