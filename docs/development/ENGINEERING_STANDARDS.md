# Engineering Standards

## Delivery And Code Organization

Freeze product, architecture, and security decisions in their owning documents
before broad implementation. Build one reviewable vertical slice at a time;
do not create speculative scaffolding to imitate planned architecture. Inspect
existing dependencies and conventions before adding packages.

Rust types own identifiers, authorization decisions, operation state, and error
classes. Use typed domain, application, and adapter errors; map third-party
failures before they cross an application port. Stable external error DTOs are
renderer-safe and audience-specific. Panic is a supervised defect path, never a
normal cross-boundary result.

Svelte 5 uses typed context and focused `.svelte.ts` reactive modules. Stores are
reserved for justified asynchronous subscription boundaries. Features call typed
ports; designated adapters alone invoke Tauri. Project-owned component wrappers
and design tokens protect the renderer from third-party implementation details.

## Testing And Validation

Use the narrowest meaningful check while iterating, then expand according to
risk. Pure logic needs unit tests; native behavior needs validation in the real
Tauri App; critical workflows need focused desktop end-to-end evidence. Browser
previews, screenshots, and mocked frontends cannot prove native integration,
secure IPC, secret handling, or window lifecycle.

Fixtures never contain real credentials, hosts, private keys, tokens, or
sensitive server output. Debug and fixture adapters are explicit test-only
surfaces and must not ship as product behavior.

Security-patched vendored dependencies retain their upstream license, source
identity, exact baseline version, patch rationale, and focused regression tests.
Dependency resolution must prove that production uses the patched path. An
upgrade cannot remove or bypass a patch until the upstream version is shown to
provide equivalent behavior. For SSH, generated loopback servers and synthetic
credentials prove protocol order without placing real endpoints, keys, passwords,
or terminal output in source, logs, screenshots, or persistent test artifacts.

## Review Discipline

Before completion, verify primary ownership, indexes and links, focused
guardrails, and relevant test evidence. Report commands run, outcomes, and
checks that were not run. Preserve unrelated working-tree changes and do not
advance from requirements into implementation without an explicit instruction.

## UI Engineering

Use a maintained component library for ordinary controls. Custom controls,
focus behavior, menus, dialogs, and overlays need a documented reason when the
shared component contract cannot meet the need. Validate keyboard operation,
focus restoration, accessibility, density, light/dark themes, localization, and
native lifecycle in the real App.
