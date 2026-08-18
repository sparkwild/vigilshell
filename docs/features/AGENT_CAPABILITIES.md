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

## Risk And Availability

Capabilities carry a risk tier and default state. High-risk entries are disabled
until the Human confirms their exact Scope configuration. Initial release exposure
may be read-only, but the authorization architecture is not read-only-only:
later controlled writes use the same catalog, Scope, Session, confirmation,
dispatch, redaction, and audit path.

Prohibited entries include raw shell execution, arbitrary URL access, arbitrary
file discovery, raw SSH/SFTP handles, real endpoint disclosure, and credential
access. Full Access remains an explicit versioned set; it never becomes a
wildcard that absorbs newly introduced capabilities.

## Result Semantics

Results distinguish complete, partial, denied, unavailable, cancelled, and
outcome-unknown states with stable machine-readable recovery details. An effectful
operation whose remote outcome is uncertain is never silently replayed.
