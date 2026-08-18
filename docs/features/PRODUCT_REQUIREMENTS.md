# Product Requirements

## Human Operations

The Human Lane provides the complete server-management experience: connection
catalogue and trust decisions, concurrent server and terminal workspaces, SFTP
files and transfers, tunnels and diagnostics, server insights, logs, services,
controlled changes, settings, artifacts, and portability. Human interaction is
not reduced to the capability set exposed to an Agent.

## Connection Configuration

The initial editable Connection Profile contains a Human-visible display name,
SSH host, port, username, enabled state, and one native-owned password binding.
VigilShell generates the Managed Server, Connection Profile, and Credential
identifiers; the renderer never supplies or edits those identities. A newly
created profile starts with security revision 1 and an unconfigured credential
reference, then launches the native credential flow as a separate settlement.
Cancelling password capture retains the profile in an explicitly unconfigured
state instead of inventing a credential or rolling back unrelated metadata.

The Human may view and edit the display name, host, port, username, and enabled
state in the Human Lane. Host and username are sensitive Human configuration
data: they may appear only in the typed Connection editor projection and active
Human workspace, not in Agent views, generic catalogue DTOs, logs, telemetry,
audit messages, or error payloads. Passwords and Credential references never
enter the renderer.

Every update and delete carries the last observed record version. A stale write
fails with a visible conflict and never overwrites newer state. Host, port,
username, or credential-binding changes advance the security revision and make
older Host Trust unusable; display-name and enabled-state changes advance only
the record version. Deleting a Connection requires an explicit Human decision,
removes its bound platform credential, then conditionally deletes the exact
record version. If storage settlement fails after credential removal, the app
reports that the profile remains but must be reconfigured; it does not pretend
the deletion completed.

## Host Trust And Human Terminal

The Human explicitly probes a configured Connection before first use. The probe
performs only SSH key exchange, obtains no credential, starts no authentication,
and shows the canonical SHA-256 Host Key fingerprint in the Human Lane. First
use and a changed key require a local confirmation bound to the exact observed
key and current security revision; a changed key receives a stronger warning.
VigilShell never applies silent trust-on-first-use or replaces trust because a
connection attempt failed.

Opening a Human terminal always rechecks the live Host Key before password
authentication. A stored trust record is necessary but not sufficient. The app
reports Host Key mismatch, authentication rejection, PTY rejection, shell
rejection, disconnect, and local close as distinct safe outcomes. It exposes no
credential or raw SSH handle to the renderer.

The terminal carries ordered raw bytes, keyboard input, and size changes through
typed, bounded native controls. Reconnect is an explicit Human action that runs
fresh admission and creates a new session and PTY epoch. It never automatically
replays input, login scripts, or an unconfirmed command. A Connection with a live
native terminal session cannot be deleted.

## Agent Authority Model

Agents use an Agent Scope and a Human-approved Agent Session; neither is an SSH
session, PTY, shell, nor ambient credential grant. Scope resolution binds a
versioned explicit capability set, resource range, constraints, and validity.
Session narrowing is allowed; expansion is not automatic.

Every Agent request remains subject to policy, risk validation, execution guards,
redaction, and audit. The global Agent switch, Scope mutation, Session revocation,
and in-flight settlement must immediately constrain unused authority. Multiple
independent Direct invocations may run concurrently when real capacity and
resource-conflict rules allow them; the App must not add a hidden batch queue.

## Scope Configuration

- Diagnostic Access, Routine Operations Access, and Full Access resolve to
  versioned explicit capability sets; any template edit becomes Custom Access.
- New Scope UI may preselect Diagnostic Access only as an uncommitted draft.
- High-risk capabilities default off. Enabling or expanding them requires a
  consequence warning and a second confirmation bound to the exact Scope.
- Resource-range selection is independent from the access-template choice.
- Directory grants are recursive canonical-root boundaries. They never grant a
  parent, sibling, symbolic-link escape, exclusion rule, or inferred discovery.

## Agent File Retrieval

The single typed file-retrieval capability supports `auto`, `inline`, and
`download` delivery. Inline content is bounded and redacted. Download preserves
original remote bytes only in the App-created `${user_home}/VigilShell` resource
root or a Human-selected custom directory; the Agent never chooses a local path.
Successful downloads return an ArtifactId and their exact final absolute path,
without duplicating original bytes in the result. Built-in file type, encoding,
size, and transfer defaults are versioned Rust policy, not request or Settings
overrides.

## Product Quality Baseline

Security-sensitive failures fail closed and remain auditable. The renderer sees
safe view models and opaque identifiers rather than credentials, raw connection
handles, or unrestricted execution primitives. Human and Agent lanes have
separate entrypoints, sessions, capability models, and authorization paths.
