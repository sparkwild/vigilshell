# Release And Distribution Standard

## Distribution Model

Direct download is the canonical distribution channel. macOS is qualified first,
followed by Windows and Linux. Package managers and app stores are secondary
channels and never replace direct-package stability requirements.

Stable, prerelease, and nightly channels have distinct artifact identities,
update eligibility, retention, and revocation rules. A platform is supported
only after its package, installation, update, uninstall, native integration,
credential integration, terminal behavior, and critical workflows pass release
qualification.

## Artifact And Supply-Chain Rules

Release artifacts require platform-appropriate signing, update trust, provenance,
and revocation controls. Build and publication pipelines keep signing material
outside source and renderer exposure. A revoked or invalid update must fail
closed and be visible through safe release diagnostics.

## Ownership

Release engineering owns channel policy, artifact matrix, signing, update
metadata, publication, retention, and emergency revocation. Product features do
not silently add a distribution path or weaken platform qualification.
