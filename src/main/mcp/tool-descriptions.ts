import type { AgentCall } from '@contracts';
export const descriptions: Record<AgentCall['tool'], string> = {
  connections_list:
    'List connections available to this access entry and their currently running terminal tabs (tabs). Choose a specific tab only when the user asked to continue in it.',
  session_manage:
    'Open, inspect or close an authorized session. By default, opening creates a new visible terminal and file workspace after authorization. To take over a user-specified existing terminal, pass its tab_id from connections_list; do not guess between tabs. The ready context includes terminal.id and terminal.control.leaseId for terminal_manage workspace read_tab/write_tab. Opening does not grant additional permissions. Closing releases control while keeping the terminal for manual use.',
  capabilities_get:
    'Inspect configured permission, SSH/SFTP environment support and current availability for individual operations',
  server_inspect: 'Read server overview or process information without changing server state',
  command_run:
    'Execute independently checked commands or native standalone-script files over SSH. These tasks do not run inside the visible shared shell or inherit its cwd/environment; results remain in tasks_query and agent activity, not the terminal. For visible interactive shell work with operate permission, use terminal_manage workspace write_tab/read_tab instead, including shell file commands. Read-only accepts only the supported simple-command allowlist. Operate checks explicit high-risk commands. It does not sandbox program internals. Standalone scripts run directly unless view-before-execution is enabled. When enabled, the user views the content and confirms with identity verification. Changed content is rejected before launch. File identity and permission-version changes are not extra restrictions. The last content check is not atomic isolation',
  terminal_manage:
    'Use action workspace with command read_tab (tab_id, lease_id, after cursor) and write_tab (tab_id, lease_id, request_key, text) on the terminal returned by session_manage.get. Raw input and takeover require operate permission; read-only sessions can inspect output and use checked command_run or structured tools. list_tabs discovers running tabs; select an existing tab via session_manage.open tab_id. Identify the user-requested tab; do not guess between multiple tabs. This reuses the real shell and locks manual input and file changes in that tab. Release with session_manage.close; it preserves the terminal for the user. After user release, do not retry or reacquire automatically: a later user task requires a new session_manage.open with the requested tab_id. read_tab returns only actual output from this shell, not structured tool feedback or proof of command completion; inspect output before proceeding. Task open/write/resize/close actions run a separate managed PTY whose output is available through tasks_query and agent activity, not the shared terminal. Use workspace write_tab for visible shell work, including file commands. File tools use the file pane for feedback. Raw PTY input is not covered by complete command or script checks. A sent-input receipt does not prove remote application.',
  files_read:
    'List, stat or read an explicitly named remote path. Access uses the connection account and actual path/link/version checks',
  files_write:
    'Write or edit remote files with explicit content, write mode and content-version checks',
  files_manage:
    'Create directories, copy, move, link or change file attributes with explicit target, link and version checks',
  files_remove:
    'Remove an explicitly named remote path. Recursive deletion must be explicit and respects system protection',
  files_transfer:
    'Upload or download files. Read-only permission prohibits upload. Use the local scope and relative path from the session policy. When both uploadRoots and downloadRoots are empty, local scopeId is "local" and local.path is relative to the filesystem root (e.g. Users/me/file for /Users/me/file). Directory limits, when configured, apply to both directions. App data and credential protections remain enforced',
  services_query: 'Query native system or current-user services, status, logs or definitions',
  services_control:
    'Start, stop, restart, reload, enable or disable a native service. Enable and disable do not start or stop it. Operate permission is required',
  services_define:
    'Create, update or remove a native unit or named drop-in using object-version checks. Does not implicitly start, stop or restart services',
  packages_query: 'Read installed native package information',
  packages_manage:
    'Install, upgrade or remove native packages, including explicit local package files bound to a SHA-256 digest. Native dependencies and maintenance scripts may change the host or start services. Command target checks do not constrain these effects',
  package_sources_query:
    'Read configured software sources and security metadata without returning credentials',
  package_sources_manage:
    'Create, update, remove, enable or disable a software source. Public-key changes must be explicit. Native signature checks remain enabled',
  system_settings_manage:
    'Change hostname, timezone, locale or supported static DNS through fixed native operations. Managed DNS requires a supported adapter',
  power_control:
    'Request reboot or power off. Acceptance or connection loss does not prove the power operation completed',
  tasks_query:
    'List tasks or read one task and its paginated stream or terminal screen. Query the original task after a timeout. Do not replay an unknown operation',
  task_cancel:
    'Request cancellation of a task owned by this access entry. A stop request does not prove termination, rollback or that a native service stopped',
};
export const readOnlyTools = new Set<AgentCall['tool']>([
  'connections_list',
  'capabilities_get',
  'server_inspect',
  'files_read',
  'services_query',
  'packages_query',
  'package_sources_query',
  'tasks_query',
]);
