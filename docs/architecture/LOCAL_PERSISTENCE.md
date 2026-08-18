# Local Persistence Architecture

## Storage Boundary

SQLite stores private durable application state such as connection metadata,
Scope and Session records, policy state, journals, and protected audit indexes.
Platform credential providers store credential material; application storage and
the renderer retain opaque references only.

The user-visible resource root contains only `downloads/`, `exports/`, and
`backups/`. It is not a substitute for private application storage. Changing the
resource root affects future artifacts only and never scans, relocates, or
relinks existing artifacts.

## Durability And Concurrency

Storage owners define transaction boundaries, migration sequencing, recovery,
and concurrent access rules. Durable records distinguish authoritative audit
evidence from convenience read models. Operations settle with enough state to
report known success, known failure, cancellation, or outcome-unknown without
inventing a remote result.

Explicit application Quit first closes native mutation admission and drains
already admitted operations. The storage shutdown barrier then requires SQLite
autocommit state and flushes its page cache before native termination is allowed;
an open transaction or flush failure is never reported as a successful flush.

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

Only a native AppKit secure-input prompt may introduce or replace password
material. A renderer may request that flow for a selected opaque Connection
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
