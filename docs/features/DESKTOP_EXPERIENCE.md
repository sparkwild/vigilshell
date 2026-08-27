# Desktop Experience

## Product UI Source Of Truth

The running Tauri desktop App is the sole product UI source of truth. Browser
previews, Storybook-like shells, and mocked renderer-only implementations cannot
prove native lifecycle, IPC, secret handling, terminal behavior, or window
integration.

## Visual Direction

VigilShell uses a Modern Native Desktop Workbench direction: restrained,
professional, compact, and information-dense. Terminal, files, logs, services,
and active work remain visually central. The product avoids marketing-card grids,
monitoring-wall decoration, cyberpunk styling, ornamental motion, and a
chat-shaped primary workbench.

Light and dark themes preserve the same hierarchy and density. Agent state is
discoverable where relevant but never dominates Human server work. Theme choices
are `system`, `light`, and `dark`; language choices are `system`, `zh-CN`, and
`en`.

## Desktop Interaction

- The primary workbench does not repeat the product name inside renderer chrome,
  and the macOS primary window keeps its visible native title empty. Bundle
  identity, product-owned menu and menu-bar naming, native traffic-light controls,
  the window drag surface, and accessibility labels remain unchanged.
- Visible workbench headers omit internal architecture labels and redundant
  explanations when location and controls already communicate their purpose.
  Terms such as `Human Lane` and `Connection catalog` do not appear as visible
  header copy; concise user-facing accessibility names remain available.
- Default product surfaces omit internal UUIDs, Profile and Server identifiers,
  record or security revision numbers, opaque runtime or session terms, and raw
  lifecycle or status telemetry. Debug-only diagnostics may expose bounded,
  non-secret opaque values when needed for development. Human-useful SSH hosts,
  ports, Host Key fingerprints, changed-key warnings, security decisions, and
  actionable connection or terminal states remain visible.
- VigilShell remains resident when the last management window closes; on macOS
  the menu-bar item is the persistent product presence.
- The initial product has one primary workbench. Terminal and server concurrency
  use tabs and split panes rather than separate workbench windows.
- The left catalogue keeps its existing workbench location and expands into a
  compact `Project → Server → Connection` hierarchy. Selecting a Connection
  activates its Server Workspace in place in the centre pane. The right Inspector
  is contextual: selecting a Project or Server shows its Human management fields
  without implicitly activating a Connection, while selecting a Connection shows
  its editor, Host Trust, credential, and terminal actions.
- A Server Workspace shows only implemented product surfaces. The initial slice
  keeps Terminal central and does not render placeholder navigation for Files,
  Insights, Logs, Services, or other later modules. Human-visible Server and
  Connection names label the workspace and terminal tabs; internal identifiers
  are never fallback copy.
- Normal primary-window close always hides and retains its WebView; it does not
  stop the resident Core. An active Human terminal remains owned by that hidden
  display context. Unexpected renderer loss is reported as display-context loss
  and never pretends to restore a PTY or replay input.
- The menu-bar item can reopen the existing primary workbench or request an
  explicit Quit. Closing the window is never interpreted as that Quit request.
- On macOS, VigilShell disables Tauri's default application menu. Only the
  primary instance installs the product-owned application menu and menu-bar
  controller during trusted setup.
- Starting VigilShell again is an activation request for the resident primary
  instance. It restores, unminimizes, and focuses that instance's existing
  workbench; it never creates a second Core, workbench, menu-bar controller,
  SQLite owner, or Keychain owner.
- Activation is accepted only while the trusted lifecycle is Running; shutdown
  never reopens the workbench. The primary-instance claim, listener readiness,
  bounded activation, and recovery mechanics are owned by
  [SYSTEM_ARCHITECTURE.md](../architecture/SYSTEM_ARCHITECTURE.md).
- Native menus, notifications, keyboard focus, dialogs, and accessibility must
  use trustworthy Core state. A notification may navigate or request attention;
  it never grants authority or settles an operation.

## Human And Agent UX Boundary

Human confirmation surfaces identify the affected Scope, target, risk, and
normalized operation. They cannot substitute for native enforcement. Agent views
receive only redacted safe data and opaque identifiers; changes to Scope,
Session, or global Agent availability are visibly attributable in Agent Trail.
