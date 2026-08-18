# Transport And Terminal Architecture

## SSH And SFTP Boundary

The Core establishes SSH and SFTP connections through project-owned transport
ports. It resolves routing, proxies, and jump hosts; verifies host identity;
uses purpose-bound credential references; and applies explicit cryptographic,
cancellation, recovery, and transfer rules. Neither a renderer nor MCP receives
a raw transport handle.

Human and Agent transport ownership is separate. A Human terminal uses a Human
connection lease. An Agent invocation may receive only a session-bound,
operation-bound Agent lease after authority is enforced. No physical reuse grants
ambient remote state, a working directory, command history, or shell access.

## Host Trust Enrollment

Host Trust enrollment is a two-step native workflow. A renderer asks Rust to
probe an opaque Connection Profile at its observed record version. Rust reloads
the enabled profile and performs a bounded SSH key exchange with the same Host
Key algorithm policy used by a real connection. The probe captures a canonical
`SHA256:<base64-unpadded>` fingerprint and deliberately terminates during Host
Key checking. It never asks the Credential Broker, authenticates, or opens a
channel.

First-seen and changed keys create a random opaque pending decision in a Rust-
owned registry. The registry holds at most 32 decisions, retains only the latest
for each profile, and expires entries after 60 seconds. A renderer may display
the key type, observed fingerprint, previous fingerprint, and decision state,
but may return only the pending ID. It cannot submit a fingerprint, route, trust
record, or security revision.

`changed` covers either a different observed Host Key or a stale revision-bound
trust context. Previous and observed fingerprints can therefore be identical;
the Human warning describes a changed Host identity or trust context and must not
claim that key bytes changed when they did not.

Confirmation consumes the pending ID, reloads the profile, and conditionally
writes Host Trust only if the profile remains enabled and the original security
revision remains current. The SQLite transaction owns the compare and write. A
display-name-only record-version change does not reinterpret the observed route,
while a disabled profile fails closed and any route, account, or credential-
binding revision change fails closed as stale. Matching current trust needs no
replacement confirmation. Closing a confirmation explicitly discards the pending
decision; expiration or replacement makes it unusable.

## Human Terminal Admission

Opening a Human terminal follows this order:

1. Validate the Connection Profile identifier and terminal dimensions before
   invoking infrastructure.
2. Load the exact Connection Profile and reject a missing or disabled profile.
3. Require stored Host Trust evidence bound to the same profile and security
   revision.
4. Ask the Credential Broker for a short-lived lease whose purpose is Human
   Terminal authentication.
5. Pass the trusted route, expected host fingerprint, credential lease, and
   terminal dimensions to the Human-only Transport Port.
6. Return a renderer-safe view containing opaque session, server, and profile
   identifiers only.

Credential access never occurs before Connection and Host Trust admission.
Changing a route, account, credential binding, proxy or jump chain, or another
security-relevant profile field advances the profile security revision and makes
older trust evidence unusable. The transport adapter still compares the observed
host identity with the expected trusted fingerprint during the real handshake;
stored trust alone does not authorize a mismatched host.

Missing, disabled, stale-trust, credential, and transport failures remain
distinct typed outcomes. None may fabricate a session, retry an effectful step
silently, or expose endpoint, username, credential, lease, or transport details
to a renderer-facing result.

For the initial macOS password path, the lease contains password bytes only
inside the trusted Rust process. It has no renderer-serializable form, does not
implement cloning, and is consumed by value at the Human transport port. Its
debug representation is permanently redacted, and its buffer is zeroized after
authentication succeeds or fails. Credential provisioning and removal are
separate Human operations; neither authorizes transport before the normal
Connection and Host Trust admission sequence.

The real transport repeats Host Key verification before authentication. It then
performs password authentication, opens a session channel, requests a PTY with a
reply, and requests a shell with a reply. A session identity is created only
after all three remote settlements succeed. Probe, connect, authentication, PTY,
and shell setup have explicit timeouts. Established idle shells use a 30-second
keepalive and disconnect after three unanswered keepalives rather than applying
a short inactivity timeout.

The initial password transport uses the repository-patched `russh` dependency.
Password state and authentication staging buffers are zeroized on success,
failure, and drop; protocol, authentication, and terminal payload
bytes are never written to library logs. Host Key mismatch occurs before password
authentication and remains distinguishable from authentication, PTY, shell,
capacity, disconnect, and transport failures without exposing sensitive details.
Dropping only the password-authentication future is not a reusable cancellation
boundary. A timeout or cancellation sends a best-effort disconnect and destroys
the entire unfinished client handle and attempt; that handle is never reused.

## Human Terminal Runtime

Rust owns the Human PTY session and its lifecycle. A project-owned xterm adapter
renders ordered raw byte streams and sends typed input, resize, credit, and
lifecycle control through the Tauri boundary. Clipboard, paste, links,
scrollback, logging, encoding, and renderer loss all follow explicit safety and
truthfulness rules.

Each live terminal has one async Rust owner task that exclusively owns the SSH
handle and channel halves. Shared state stores only opaque session/profile IDs
and a bounded control sender; it never holds a registry lock across network
awaits. The initial capacity is 32 sessions globally, 4 per profile, and 64
queued controls per session. Capacity rejection is explicit and does not create
a hidden queue.

The originating WebView receives one ordered Tauri IPC Channel. Terminal data is
sent as raw bytes and lifecycle state as typed JSON; it is neither coerced to
UTF-8 nor broadcast application-wide. A session starts with 256 KiB of output
credit, caps outstanding credit at 1 MiB, and emits chunks no larger than 32 KiB.
When credit is exhausted, the owner stops polling remote channel output so SSH
windowing applies backpressure. The renderer replenishes credit only after the
xterm write callback completes.

Input is limited to 64 KiB per control. Resize uses the domain terminal bounds,
and one credit grant cannot exceed 1 MiB. EOF, close, exit status, exit signal,
transport loss, and local close remain ordered lifecycle settlements. A failed
IPC delivery or lost display context closes the native session and removes it
from the registry. Native Connection deletion checks this registry before any
credential removal and rejects a profile with a live session.

The runtime reserves opening capacity before network work. After successful
authentication, PTY, and shell setup, the first lifecycle message repeats the
new session ID. If that first delivery fails, the owner immediately closes and
unregisters the session so a lost invoke response cannot create an invisible
live terminal. Window or WebView destruction proactively closes all sessions for
that display context instead of waiting for another output send to detect loss.

Terminal open and Connection delete use reservations under the same registry
lock. A delete reservation is acquired before credential removal and remains
owned until the complete credential-and-SQLite settlement; it blocks new opens
for that profile. An open reservation blocks deletion until it either becomes a
live session or fails and is released. A one-time `has_live` check is not an
acceptable concurrency control. The delete token remains owned by the blocking
credential operation so cancellation of its outer Tauri command cannot release
the reservation while credential removal is still running.

A normal primary-window close hides and retains its WebView; an active terminal
continues to belong to that hidden display context. An explicit terminal close
settles and removes that native session. Unexpected WebView destruction is an
interruption, not session restoration. Disconnect, transport loss, explicit
reconnect, and explicit Quit have distinct settlements. Explicit Quit closes
terminal admission before closing live owners and waiting for opening, deleting,
and live registrations under the application shutdown deadline. Reconnect runs
fresh admission, creates a new session and PTY epoch, and never replays prior
input or login actions.

## File Transfer Boundary

SFTP browsing and transfer remain Core-owned. Original bytes may reach a local
artifact only through an authorized transfer path. Inline text, UI projections,
diagnostics, and audit are bounded and redacted according to their audience.
