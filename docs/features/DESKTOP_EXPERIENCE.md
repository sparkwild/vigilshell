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
- VigilShell remains resident when the last management window closes; on macOS
  the menu-bar item is the persistent product presence.
- The initial product has one primary workbench. Terminal and server concurrency
  use tabs and split panes rather than separate workbench windows.
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
- Secondary-launch arguments and the secondary working directory are discarded.
  They never enter the renderer, logs, persistence, or an authority decision.
  The secondary sends only an empty activation signal to the primary.
- A synchronous per-user primary-instance claim completes before desktop runtime
  and business setup. After obtaining the claim, the primary removes any stale
  official-listener socket before constructing the desktop runtime; cleanup
  failure stops startup.
- Before any window or business setup, a native readiness gate sends an empty
  payload through the official listener and requires its acknowledgement. If the
  owner crashes during startup, another launch may recover the claim within the
  same bounded five-second window; readiness timeout or I/O failure stops startup
  without creating a partial second instance.
- A secondary performs listener connect, empty-payload write, and notification
  inside one worker governed by that same deadline, so a blocked native I/O call
  cannot extend startup beyond the bound.
- Activation acquires the same resident operation admission used to linearize
  `Running` to `Quitting`. `Quitting` and `ExitReady` reject it rather than
  reopening the workbench or delaying settlement.
- Native menus, notifications, keyboard focus, dialogs, and accessibility must
  use trustworthy Core state. A notification may navigate or request attention;
  it never grants authority or settles an operation.

## Human And Agent UX Boundary

Human confirmation surfaces identify the affected Scope, target, risk, and
normalized operation. They cannot substitute for native enforcement. Agent views
receive only redacted safe data and opaque identifiers; changes to Scope,
Session, or global Agent availability are visibly attributable in Agent Trail.
