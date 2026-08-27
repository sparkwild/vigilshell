# Product Requirements

## Human Operations

The Human Lane provides the complete server-management experience: connection
catalogue and trust decisions, concurrent server and terminal workspaces, SFTP
files and transfers, tunnels and diagnostics, server insights, logs, services,
controlled changes, settings, artifacts, and portability. Human interaction is
not reduced to the capability set exposed to an Agent.

The initial Human product scope covers M01 through M11 and M15. M12 through M14
remain outside this delivery scope until the Agent Lane is explicitly resumed.
The initial professional terminal scope includes the real password-backed SSH
and Host Trust loop, safe copy and paste, bounded scrollback and search, split
panes, keyboard shortcuts, and Unicode/IME behavior. Command snippets, terminal
broadcast, selectable encodings, and session logging remain later product
capabilities rather than initial-version acceptance requirements.

## Connection Configuration

The initial editable Connection Profile contains a Human-visible display name,
SSH host, port, username, enabled state, and one native-owned password binding.
VigilShell generates the Managed Server, Connection Profile, and Credential
identifiers; the renderer never supplies or edits those identities. A newly
created profile starts with security revision 1 and an unconfigured credential
reference, then launches the native credential flow as a separate settlement.
Cancelling password capture retains the profile in an explicitly unconfigured
state instead of inventing a credential or rolling back unrelated metadata.

A Managed Server is an independent, stable server identity and may own multiple
Connection Profiles for different routes, accounts, or credentials. Projects
organize Managed Servers and Connection Profiles for Human navigation; they do
not merge those identities or create authority semantics.

Each Managed Server has its own Human-visible name and optimistic record version.
The global New action creates one Server and its first Connection atomically;
the initial Server name copies the first Connection name but remains independently
editable. Adding another Connection is an explicit action in the active Server
Workspace. VigilShell never infers Server identity from host, username, or route.

A Managed Server may belong to at most one Project, and its Connections inherit
that navigation placement. Servers without a Project appear under a virtual
`No project` group. Deleting a Project unassigns its Servers and deletes no Server,
Connection, credential, trust record, or remote state. Deleting the last Connection
retains the empty Server; Server deletion is a separate explicit action and is
allowed only when the Server has no Connections.

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
and in-flight settlement must immediately constrain unused authority.

Access-template resolution, Scope semantics, stable Capability identities,
invocation concurrency, file grants, governed retrieval, and runtime availability
gates are defined once in [AGENT_CAPABILITIES.md](AGENT_CAPABILITIES.md).
Documenting one of those contracts does not make it available before its
production handler and every registration gate are complete.

## Product Quality Baseline

Security-sensitive failures fail closed and remain auditable. The renderer sees
safe view models and opaque identifiers rather than credentials, raw connection
handles, or unrestricted execution primitives. Human and Agent lanes have
separate entrypoints, sessions, capability models, and authorization paths.
