# Local Persistence Architecture

## Storage Boundary

SQLite stores private durable application state such as connection metadata,
Scope and Session records, policy state, journals, and protected audit indexes.
Platform credential providers store credential material; application storage and
the renderer retain opaque references only.

The desktop resolves one immutable private-data layout before business setup.
The Tauri local-data root owns `vigilshell.sqlite3` for production,
`vigilshell-debug.sqlite3` for development, and the per-user single-instance
lock. Windows additionally owns isolated
`secure/<environment>/credentials/v1/` directories for Current-User DPAPI
ciphertext. Debug and test-support builds may replace the entire root through
`VIGILSHELL_DESKTOP_DATA_DIR`; production rejects that override. A legacy
single-database path override is not accepted.

Application-owned roots must be absolute and must not resolve to a symlink,
reparse point, or wrong filesystem type. On macOS private directories use mode
`0700` and SQLite files use `0600`. The layout is resolved once and injected;
commands and services do not independently call Tauri path APIs or assemble
private storage paths.

Projects, Managed Servers, and Connection Profiles are separate records. A
Managed Server has a Human-visible name, positive record version, and an optional
Project reference. A Connection Profile has one required Managed Server reference.
Project deletion sets that reference to no Project; Server deletion is restricted
while any Connection Profile references it. Connection deletion never implicitly
deletes its Server.

The user-visible resource root contains only `downloads/`, `exports/`, and
`backups/`. It is not a substitute for private application storage. Changing the
resource root affects future artifacts only and never scans, relocates, or
relinks existing artifacts.

The default resource root is `${user_home}/VigilShell`, with `user_home` resolved
by the native platform adapter. Domain code never expands a literal `~` or falls
back to the working directory, a temporary directory, or a broader path when the
root cannot be created. A Human-selected custom root changes only future artifact
destinations and never relocates private application state.

## Durability And Concurrency

Storage owners define transaction boundaries, migration sequencing, recovery,
and concurrent access rules. Durable records distinguish authoritative audit
evidence from convenience read models. Operations settle with enough state to
report known success, known failure, cancellation, or outcome-unknown without
inventing a remote result.

SQLite uses one Mutex-protected connection. Product writes use `BEGIN IMMEDIATE`;
reads use the same connection and retain one consistent read snapshot. The
connection enables foreign keys, WAL, FULL synchronous durability, untrusted
schema protection, cell-size checking, a bounded busy timeout, and incremental
auto-vacuum capability.

The current storage generation is identified independently from migration
numbering through SQLite application ID, `schema_metadata`, and a generation
string. Fresh generation databases start again at `user_version = 1`. The
one-time legacy adapter accepts only the exact previous v1, v2, or v3 shapes,
validates every decoded domain value, and converts only the database selected by
the current production or development layout. Unknown, ahead, partial, or
corrupted schemas are never overwritten or deleted. Credential references are
preserved exactly and Keychain items are not migrated.

Startup performs a quick integrity and foreign-key check. A singleton runtime
record marks the database dirty only after validation; if the previous run did
not reach clean shutdown, the next startup also performs a full integrity check.
Explicit application Quit first closes native mutation admission and drains
already admitted operations. The storage shutdown barrier requires SQLite
autocommit state, optimizes the schema, checkpoints WAL, writes the clean marker,
and truncates WAL before native termination is allowed. An open transaction,
busy checkpoint, or flush failure is never reported as successful shutdown.

Each Connection Profile has two independent positive counters. `record_version`
advances on every committed update and is the optimistic-concurrency token for
update and delete. `security_revision` advances only when a route, account,
credential binding, proxy, jump chain, or another security-relevant field
changes. Display-name and enabled-state edits do not invalidate Host Trust.
Storage mutations use conditional SQL inside one transaction; callers cannot
choose either next counter value or use an unrestricted upsert as a product
mutation API.

Host Trust stores only the canonical SHA-256 fingerprint beside the Profile ID
and the exact security revision that was observed. A production trust mutation
is a conditional `BEGIN IMMEDIATE` transaction: it reloads the Profile and writes
or replaces trust only when that revision still matches the native pending Host
Key decision. The renderer cannot supply the fingerprint or revision. Expired,
consumed, replaced, missing, or stale decisions produce no durable write.

New Connection creation receives trusted-native generated Managed Server,
Connection Profile, and Credential identities and commits record version 1 and
security revision 1. The SQLite profile row stores the Human display name and
opaque Credential reference. The Human editor may receive host and username in
a dedicated renderer-safe projection, while list summaries and errors remain
opaque and redacted.

Creating a new Server and its first Connection is one immediate transaction.
Adding a Connection to an existing Server reloads that exact Server inside the
trusted boundary and creates only a new Profile and Credential identity. The
renderer may return an opaque Server selection previously projected by Rust, but
cannot invent a Server identity or cause heuristic host-based merging.

The first normalized Server migration creates one Managed Server for each exact
distinct legacy `server_id`; it never groups records by host, username, or route.
The Server name comes from the display name of the lexicographically first Profile
for that ID, while every Connection display name remains unchanged. Projects start
unassigned. Migration rebuilds the Profile foreign-key boundary and preserves
Profile IDs, Credential references, record/security versions, Host Trust rows, and
routes in one transaction; invalid or incomplete input rolls the whole migration
back rather than clearing or partially rewriting the database.

## Secret And Failure Rules

Secret values stay in the trusted native boundary and have minimal in-memory
lifetime. Encryption, platform protection, and local threat assumptions are
documented as boundary properties rather than exposed through renderer settings.
Failures fail closed, preserve safe correlation, and never fall back to a working
directory, temporary directory, or broader path when a protected root is absent.

On macOS, the initial Human SSH password path is one generic-password Keychain
item per opaque `CredentialRef`. Every create, update, read, and delete request
uses the data-protection Keychain and explicitly selects the non-synchronizing
store. Password items use the platform's foreground, unlocked-device protection;
VigilShell does not weaken that policy or silently retry through another store.

On Windows, the same opaque `CredentialRef` selects one file under the active
credentials namespace. The filename is a SHA-256 digest, not the reference
itself. A versioned envelope binds its material kind and locator digest to
Current-User DPAPI ciphertext; DPAPI UI and machine-wide protection are
forbidden. Writes use a same-directory temporary file, flush, and atomic
replacement, while every read revalidates file type, size, envelope, locator,
and protected payload integrity. Windows Credential UI captures the password
without persisting it through Credential Manager.

Only a native AppKit secure-input prompt may introduce or replace password
material on macOS; Windows uses the native password-only credential prompt. A
renderer may request that flow for a selected opaque Connection
Profile, but it never receives the password, Keychain query, or credential
reference. Cancel is a non-error settlement. Empty input, unavailable Keychain,
missing items, and denied access are typed fail-closed outcomes whose public
messages contain no host, username, secret, or platform error detail.

The Credential Broker retrieves a password only after Human Terminal admission
has validated the Connection Profile and revision-bound Host Trust. The resulting
lease owns zeroizing bytes, is moved once into the Human transport adapter, and
is cleared when that attempt settles. SQLite, logs, audit, telemetry, screenshots,
and renderer DTOs never contain credential material. Removing an item is
idempotent from the Human workflow perspective and does not delete or rewrite its
Connection Profile.

Connection deletion is an explicit cross-owner workflow. After Human approval,
the native adapter first acquires a Profile delete reservation that excludes
live and opening terminal sessions and blocks new opens. It holds that reservation
while removing the bound credential and conditionally
deletes the exact Connection record version; SQLite cascades its Host Trust row.
If the conditional delete or storage commit fails after credential removal, the
profile remains without usable credential material and the result states that
reconfiguration is required. The app never reports a successful deletion from a
partial settlement and never leaves an intentionally unreachable Keychain item
as the preferred failure mode.
