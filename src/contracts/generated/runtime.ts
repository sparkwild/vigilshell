/* Generated from Rust DTOs. Run npm run contracts:generate; do not edit. */

/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentCall".
 */
export type AgentCall =
  | ((
      | {
          action: "open"
          connection_id: string
          request_key: string
          tab_id?: string | null
          tool: "session_manage"
        }
      | {
          action: "get"
          session_id: string
          tool: "session_manage"
        }
      | {
          action: "close"
          request_key: string
          session_id: string
          tool: "session_manage"
        }
    ) & {
      tool: "session_manage"
    })
  | ((
      | {
          action: "overview"
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "server_inspect"
        }
      | {
          action: "processes"
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "server_inspect"
        }
    ) & {
      tool: "server_inspect"
    })
  | ((
      | {
          action: "command"
          command: string
          directory: string
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "command_run"
        }
      | {
          action: "script"
          arguments: string[]
          directory: string
          interpreter: ScriptInterpreter
          request_key: string
          session_id: string
          source: ScriptInput
          timeout_seconds?: number | null
          tool: "command_run"
        }
    ) & {
      tool: "command_run"
    })
  | ((
      | {
          action: "workspace"
          command: WorkspaceTerminalAction
          session_id: string
          tool: "terminal_manage"
        }
      | {
          action: "open"
          columns: number
          directory: string
          request_key: string
          rows: number
          session_id: string
          timeout_seconds?: number | null
          tool: "terminal_manage"
        }
      | {
          action: "write"
          request_key: string
          task_id: string
          text: string
          tool: "terminal_manage"
        }
      | {
          action: "resize"
          columns: number
          request_key: string
          rows: number
          task_id: string
          tool: "terminal_manage"
        }
      | {
          action: "close"
          request_key: string
          task_id: string
          tool: "terminal_manage"
        }
    ) & {
      tool: "terminal_manage"
    })
  | ((
      | {
          action: "list"
          after?: string | null
          limit: number
          request_key: string
          session_id: string
          target: string
          timeout_seconds?: number | null
          tool: "files_read"
        }
      | {
          action: "stat"
          hash: boolean
          request_key: string
          session_id: string
          target: string
          timeout_seconds?: number | null
          tool: "files_read"
        }
      | {
          action: "read"
          length: number
          offset: number
          request_key: string
          session_id: string
          target: string
          timeout_seconds?: number | null
          tool: "files_read"
        }
    ) & {
      tool: "files_read"
    })
  | ((
      | {
          action: "write"
          data: string
          expected_hash?: string | null
          mode: FileWriteMode
          request_key: string
          session_id: string
          target: string
          timeout_seconds?: number | null
          tool: "files_write"
        }
      | {
          action: "edit"
          edits: TextReplacement[]
          expected_hash: string
          request_key: string
          session_id: string
          target: string
          timeout_seconds?: number | null
          tool: "files_write"
        }
    ) & {
      tool: "files_write"
    })
  | ((
      | {
          action: "mkdir"
          parents: boolean
          request_key: string
          session_id: string
          target: string
          timeout_seconds?: number | null
          tool: "files_manage"
        }
      | {
          action: "copy"
          overwrite: boolean
          recursive: boolean
          request_key: string
          session_id: string
          source: string
          target: string
          timeout_seconds?: number | null
          tool: "files_manage"
        }
      | {
          action: "move"
          overwrite: boolean
          request_key: string
          session_id: string
          source: string
          target: string
          timeout_seconds?: number | null
          tool: "files_manage"
        }
      | {
          action: "link"
          destination: string
          request_key: string
          session_id: string
          target: string
          timeout_seconds?: number | null
          tool: "files_manage"
        }
      | {
          action: "set_attributes"
          mode?: number | null
          modified_seconds?: number | null
          request_key: string
          session_id: string
          target: string
          timeout_seconds?: number | null
          tool: "files_manage"
        }
    ) & {
      tool: "files_manage"
    })
  | ((
      | {
          action: "upload"
          local: ScopedPath
          overwrite: boolean
          remote: string
          request_key: string
          session_id: string
          tool: "files_transfer"
        }
      | {
          action: "download"
          local: ScopedPath
          overwrite: boolean
          remote: string
          request_key: string
          session_id: string
          tool: "files_transfer"
        }
    ) & {
      tool: "files_transfer"
    })
  | ((
      | {
          action: "list"
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "services_query"
        }
      | {
          action: "status"
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_query"
        }
      | {
          action: "logs"
          lines: number
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_query"
        }
      | {
          action: "definition"
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_query"
        }
    ) & {
      tool: "services_query"
    })
  | ((
      | {
          action: "start"
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_control"
        }
      | {
          action: "stop"
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_control"
        }
      | {
          action: "restart"
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_control"
        }
      | {
          action: "reload"
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_control"
        }
      | {
          action: "enable"
          expected_version?: string | null
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_control"
        }
      | {
          action: "disable"
          expected_version?: string | null
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_control"
        }
    ) & {
      tool: "services_control"
    })
  | ((
      | {
          action: "create"
          definition: NativeServiceDefinition
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "services_define"
        }
      | {
          action: "update"
          definition: NativeServiceDefinition
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "services_define"
        }
      | {
          action: "remove"
          drop_in?: string | null
          expected_hash: string
          request_key: string
          session_id: string
          target: ServiceTarget
          timeout_seconds?: number | null
          tool: "services_define"
        }
    ) & {
      tool: "services_define"
    })
  | ((
      | {
          action: "install"
          packages: PackageTarget[]
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "packages_manage"
        }
      | {
          action: "upgrade"
          packages: PackageTarget[]
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "packages_manage"
        }
      | {
          action: "remove"
          packages: PackageTarget[]
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "packages_manage"
        }
    ) & {
      tool: "packages_manage"
    })
  | ((
      | {
          action: "create"
          definition: PackageSourceDefinition
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "package_sources_manage"
        }
      | {
          action: "update"
          expected_hash: string
          patch: PackageSourcePatch
          request_key: string
          session_id: string
          source_id: string
          timeout_seconds?: number | null
          tool: "package_sources_manage"
        }
      | {
          action: "remove"
          expected_hash: string
          request_key: string
          session_id: string
          source_id: string
          timeout_seconds?: number | null
          tool: "package_sources_manage"
        }
      | {
          action: "enable"
          expected_hash: string
          request_key: string
          session_id: string
          source_id: string
          timeout_seconds?: number | null
          tool: "package_sources_manage"
        }
      | {
          action: "disable"
          expected_hash: string
          request_key: string
          session_id: string
          source_id: string
          timeout_seconds?: number | null
          tool: "package_sources_manage"
        }
    ) & {
      tool: "package_sources_manage"
    })
  | ((
      | {
          action: "hostname"
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "system_settings_manage"
          value: string
        }
      | {
          action: "timezone"
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "system_settings_manage"
          value: string
        }
      | {
          action: "locale"
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "system_settings_manage"
          value: string
        }
      | {
          action: "dns"
          configuration: DnsConfiguration
          expected_hash?: string | null
          request_key: string
          session_id: string
          timeout_seconds?: number | null
          tool: "system_settings_manage"
        }
    ) & {
      tool: "system_settings_manage"
    })
  | ((
      | {
          action: "list"
          connection_id?: string | null
          offset?: number | null
          phase?: string | null
          session_id?: string | null
          tool: "tasks_query"
        }
      | {
          action: "get"
          task_id: string
          tool: "tasks_query"
        }
      | {
          action: "output"
          cursor?: number | null
          task_id: string
          tool: "tasks_query"
          view?: OutputView | null
        }
    ) & {
      tool: "tasks_query"
    })
  | {
      tool: "connections_list"
    }
  | {
      session_id: string
      tool: "capabilities_get"
    }
  | {
      recursive: boolean
      request_key: string
      session_id: string
      target: string
      timeout_seconds?: number | null
      tool: "files_remove"
    }
  | {
      request_key: string
      session_id: string
      timeout_seconds?: number | null
      tool: "packages_query"
    }
  | {
      request_key: string
      session_id: string
      timeout_seconds?: number | null
      tool: "package_sources_query"
    }
  | {
      action: PowerAction
      request_key: string
      session_id: string
      tool: "power_control"
    }
  | {
      request_key: string
      task_id: string
      tool: "task_cancel"
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScriptInterpreter".
 */
export type ScriptInterpreter = "sh" | "bash" | "python3" | "node"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScriptInput".
 */
export type ScriptInput =
  | {
      kind: "inline"
      text: string
    }
  | {
      expected_hash?: string | null
      kind: "remoteFile"
      path: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceTerminalAction".
 */
export type WorkspaceTerminalAction =
  | {
      action: "list_tabs"
    }
  | {
      action: "takeover"
      request_key: string
      tab_id: string
    }
  | {
      action: "read_tab"
      after: number
      lease_id: string
      tab_id: string
    }
  | {
      action: "write_tab"
      lease_id: string
      request_key: string
      tab_id: string
      text: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileWriteMode".
 */
export type FileWriteMode = "create" | "replace" | "append"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ServiceManager".
 */
export type ServiceManager = "system" | "currentUser"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ServiceDefinitionContent".
 */
export type ServiceDefinitionContent =
  | {
      kind: "unit"
      text: string
    }
  | {
      kind: "dropIn"
      name: string
      text: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PackageTarget".
 */
export type PackageTarget =
  | {
      kind: "repository"
      name: string
      version?: string | null
    }
  | {
      kind: "local"
      path: string
      sha256: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PackageSourceDefinition".
 */
export type PackageSourceDefinition =
  | {
      components: string[]
      format: "apt"
      name: string
      public_key: string
      suite: string
      url: string
    }
  | {
      format: "rpm"
      name: string
      public_key: string
      url: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "OutputView".
 */
export type OutputView = "stream" | "screen"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PowerAction".
 */
export type PowerAction = "reboot" | "powerOff"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentEvent".
 */
export type AgentEvent =
  | {
      kind: "viewsChanged"
      selected_view?: string | null
    }
  | {
      kind: "activity"
      record: ActivityRecord
    }
  | {
      kind: "task"
      task: AgentTask
    }
  | {
      context: AgentContext
      kind: "context"
    }
  | {
      code: string
      connection_id: string
      id: string
      kind: "attention"
    }
  | {
      kind: "configuration"
    }
  | {
      kind: "accessChanged"
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ActivityActor".
 */
export type ActivityActor = "agent" | "user" | "system"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ActivityBody".
 */
export type ActivityBody =
  | {
      kind: "historical"
      originalKind: string
      raw: string
    }
  | {
      kind: "script"
      origin?: ScriptOrigin | null
      snapshot: ScriptSnapshot
    }
  | {
      checkpoint: ScreenCheckpoint
      kind: "screen"
    }
  | {
      bytes: number
      hash?: string | null
      kind: "transfer"
      total?: number | null
    }
  | {
      kind: "policy"
      policy: ExecutionPolicy
    }
  | {
      connectionId: string
      entryId: string
      entryName: string
      kind: "session"
      phase: ContextPhase
    }
  | {
      command?: string | null
      error?: string | null
      exitCode?: number | null
      kind: "operation"
      phase: TaskPhase
      targets: OperationTarget[]
      tool: string
    }
  | {
      delivered: boolean
      kind: "input"
      requestKey: string
      text: string
    }
  | {
      data?: string | null
      format?: ActivityOutputFormat
      kind: "output"
      stream: string
      text: string
    }
  | {
      columns: number
      kind: "resize"
      rows: number
    }
  | {
      kind: "fileChanged"
      paths: string[]
    }
  | {
      allowed: boolean
      kind: "decision"
      reason?: string | null
    }
  | {
      action: string
      kind: "control"
    }
  | {
      kind: "gap"
      reason: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "OperationLevel".
 */
export type OperationLevel = "disabled" | "readOnly" | "operate"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScopeKind".
 */
export type ScopeKind = "directory" | "file"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ContextPhase".
 */
export type ContextPhase =
  | "waitingUser"
  | "connecting"
  | "ready"
  | "closing"
  | "disconnected"
  | "closed"
  | "failed"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TaskPhase".
 */
export type TaskPhase =
  | "queued"
  | "waitingReview"
  | "running"
  | "stopping"
  | "succeeded"
  | "requestAccepted"
  | "failed"
  | "denied"
  | "cancelled"
  | "timedOut"
  | "interruptedUnknown"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileEffect".
 */
export type FileEffect = "read" | "write" | "remove"
/**
 * 区分真实进程输出与工具结果，避免将命令本身输出的 JSON 当作界面数据改写。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ActivityOutputFormat".
 */
export type ActivityOutputFormat = "text" | "structured"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "DispatchState".
 */
export type DispatchState =
  | {
      kind: "notDispatched"
    }
  | {
      attemptId: string
      attemptedAt: number
      kind: "attempted"
    }
/**
 * RequestAccepted 只确认请求被接纳，Partial 和 Unknown 均不得投影成已成功或已回滚。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionOutcome".
 */
export type ExecutionOutcome =
  | {
      kind: "notExecuted"
      reason: NotExecutedReason
    }
  | {
      exitCode: number
      kind: "completed"
    }
  | {
      kind: "requestAccepted"
    }
  | {
      kind: "stopped"
      reason: ExecutionStopReason
    }
  | {
      kind: "partial"
      reason: ExecutionStopReason
      terminationConfirmed: boolean
    }
  | {
      kind: "unknown"
      reason: ExecutionStopReason
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NotExecutedReason".
 */
export type NotExecutedReason =
  "denied" | "cancelled" | "timedOut" | "restarted"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionStopReason".
 */
export type ExecutionStopReason =
  "cancelled" | "timedOut" | "disconnected" | "restarted"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionPhase".
 */
export type ExecutionPhase =
  | "waitingReview"
  | "queued"
  | "dispatching"
  | "running"
  | "stopping"
  | "settled"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScriptReviewState".
 */
export type ScriptReviewState =
  | {
      kind: "notRequired"
    }
  | {
      binding: ScriptReviewBinding
      kind: "waitingReview"
    }
  | {
      binding: ScriptReviewBinding
      kind: "approved"
    }
  | {
      binding: ScriptReviewBinding
      kind: "rejected"
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileCommitState".
 */
export type FileCommitState = "unchanged" | "committed" | "partial" | "unknown"
/**
 * 修改类操作只接收业务参数；不能携带调用者声明的权限结论或任意维护命令。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RuntimeOperation".
 */
export type RuntimeOperation =
  | {
      configuration: DnsConfiguration
      expected_hash?: string | null
      kind: "dns_update"
    }
  | {
      kind: "system_settings_update"
      setting: SystemSetting
      value: string
    }
  | {
      action: PowerAction
      kind: "power_control"
    }
  | {
      arguments: string[]
      directory: ScopedPath
      interpreter: ScriptInterpreter
      kind: "script"
      source: ScriptSource
    }
  | {
      kind: "transfers_upload"
      local: ScopedPath
      overwrite: boolean
      remote: ScopedPath
    }
  | {
      kind: "transfers_download"
      local: ScopedPath
      overwrite: boolean
      remote: ScopedPath
    }
  | {
      command: string
      directory: ScopedPath
      kind: "command"
    }
  | {
      columns: number
      directory: ScopedPath
      kind: "terminal"
      rows: number
    }
  | {
      after?: string | null
      kind: "files_list"
      limit: number
      target: ScopedPath
    }
  | {
      hash: boolean
      kind: "files_stat"
      target: ScopedPath
    }
  | {
      kind: "files_read"
      length: number
      offset: number
      target: ScopedPath
    }
  | {
      data: string
      expected_hash?: string | null
      kind: "files_write"
      mode: FileWriteMode
      target: ScopedPath
    }
  | {
      edits: TextReplacement[]
      expected_hash: string
      kind: "files_edit"
      target: ScopedPath
    }
  | {
      kind: "files_mkdir"
      parents: boolean
      target: ScopedPath
    }
  | {
      kind: "files_copy"
      overwrite: boolean
      recursive: boolean
      source: ScopedPath
      target: ScopedPath
    }
  | {
      kind: "files_move"
      overwrite: boolean
      source: ScopedPath
      target: ScopedPath
    }
  | {
      kind: "files_remove"
      recursive: boolean
      target: ScopedPath
    }
  | {
      kind: "files_set_attributes"
      mode?: number | null
      modified_seconds?: number | null
      target: ScopedPath
    }
  | {
      destination: ScopedPath
      kind: "files_link"
      target: ScopedPath
    }
  | {
      kind: "system_get"
    }
  | {
      kind: "processes_list"
    }
  | {
      kind: "services_list"
    }
  | {
      kind: "services_status"
      name: string
    }
  | {
      kind: "services_definition"
      name: string
    }
  | {
      kind: "services_logs"
      lines: number
      name: string
    }
  | {
      action: ServiceAction
      kind: "services_control"
      name: string
    }
  | {
      kind: "packages_list"
    }
  | {
      kind: "packages_install"
      packages: PackageTarget[]
    }
  | {
      kind: "packages_update"
      packages: PackageTarget[]
    }
  | {
      kind: "packages_remove"
      packages: PackageTarget[]
    }
  | {
      definition: PackageSourceDefinition
      kind: "package_sources_create"
    }
  | {
      expected_hash: string
      kind: "package_sources_update"
      patch: PackageSourcePatch
      source_id: string
    }
  | {
      expected_hash: string
      kind: "package_sources_remove"
      source_id: string
    }
  | {
      kind: "package_sources_list"
    }
  | {
      enabled: boolean
      expected_hash: string
      kind: "package_sources_manage"
      source_id: string
    }
  | {
      definition: NativeServiceDefinition
      kind: "services_register"
    }
  | {
      definition: NativeServiceDefinition
      kind: "services_update"
    }
  | {
      enabled: boolean
      expected_version?: string | null
      kind: "services_set_enabled"
      name: string
    }
  | {
      drop_in?: string | null
      expected_hash: string
      kind: "services_unregister"
      name: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "SystemSetting".
 */
export type SystemSetting = "hostname" | "timezone" | "locale"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScriptSource".
 */
export type ScriptSource =
  | {
      kind: "inline"
      text: string
    }
  | {
      expected_hash?: string | null
      kind: "remoteFile"
      target: ScopedPath
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ServiceAction".
 */
export type ServiceAction =
  "status" | "logs" | "start" | "stop" | "restart" | "reload"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PackageManagerKind".
 */
export type PackageManagerKind = "apt" | "dnf4" | "dnf5"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "CommandCheckRequirement".
 */
export type CommandCheckRequirement = "none" | "readOnlyAllowlist" | "highRisk"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentClient".
 */
export type AgentClient = "codex" | "claudeCode" | "cursor" | "dsh" | "generic"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ProtectionCategory".
 */
export type ProtectionCategory =
  "core" | "criticalConfiguration" | "credential" | "control"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentUiRequest".
 */
export type AgentUiRequest =
  | {
      action: "activitySources"
    }
  | {
      action: "activitySummaries"
      filter: ActivitySummaryFilter
    }
  | {
      action: "approveScript"
      taskId: string
    }
  | {
      action: "rejectScript"
      taskId: string
    }
  | {
      action: "previewPolicy"
      policy: ConnectionPolicy
    }
  | {
      action: "openPolicyBrowser"
      connectionId: string
      connectionRevision: number
    }
  | {
      action: "readPolicyResources"
      browserId: string
      query: PolicyResourceQuery
    }
  | {
      action: "closePolicyBrowser"
      browserId: string
    }
  | {
      action: "retryRecording"
    }
  | {
      action: "selectLocalDirectory"
    }
  | {
      action: "hideSessionView"
      sessionId: string
    }
  | {
      action: "closeSession"
      sessionId: string
    }
  | {
      action: "recordings"
      filter: RecordingFilter
    }
  | {
      action: "snapshot"
    }
  | {
      action: "detectClients"
    }
  | {
      action: "updateSettings"
      patch: AgentSettingsPatch
    }
  | {
      action: "prepareAccess"
      client: AgentClient
      delivery: AgentDelivery
      id?: string | null
      name: string
      requestKey: string
    }
  | {
      action: "deliverAccess"
      client: AgentClient
      delivery: AgentDelivery
      id: string
    }
  | {
      action: "updatePolicySettings"
      connectionId: string
      connectionRevision: number
      expectedRevision: number
      patch: PolicySettingsPatch
    }
  | {
      action: "setPolicyAccess"
      connectionId: string
      connectionRevision: number
      enabled: boolean
      expectedRevision: number
    }
  | {
      action: "renameAccess"
      id: string
      name: string
    }
  | {
      action: "rotateAccess"
      id: string
    }
  | {
      action: "revokeAccess"
      id: string
    }
  | {
      action: "deleteAccess"
      generation: number
      id: string
    }
  | {
      action: "approveConnection"
      contextId: string
    }
  | {
      action: "task"
      taskId: string
    }
  | {
      action: "tasks"
      connectionId?: string | null
      entryId?: string | null
      offset: number
      phase?: string | null
      sessionId?: string | null
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PolicyResourceQuery".
 */
export type PolicyResourceQuery = {
  after?: string | null
  kind: "directory"
  path: string
}
/**
 * 只比较并更新所选分组，其他分组的并发修改不会被旧快照覆盖。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentSettingsPatch".
 */
export type AgentSettingsPatch =
  | {
      baseEnabled: boolean
      basePort: number
      enabled: boolean
      group: "service"
      port: number
    }
  | {
      baseRetentionDays: number
      baseStorageMib: number
      group: "activity"
      retentionDays: number
      storageMib: number
    }
  | {
      baseShowTray: boolean
      group: "tray"
      showTray: boolean
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentDelivery".
 */
export type AgentDelivery = "configuration" | "prompt"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PolicySettingsPatch".
 */
export type PolicySettingsPatch =
  | {
      field: "verifyConnect"
      value: boolean
    }
  | {
      field: "level"
      value: OperationLevel
    }
  | {
      field: "fileTransfer"
      value: boolean
    }
  | {
      field: "protectSystem"
      value: boolean
    }
  | {
      field: "scriptReview"
      value: boolean
    }
  | {
      field: "protectedPaths"
      value: ProtectedPath[]
    }
  | {
      field: "localDirectory"
      value: string | null
    }
  | {
      field: "uploadRoots"
      value: LocalDirectoryGrant[]
    }
  | {
      field: "downloadRoots"
      value: LocalDirectoryGrant[]
    }
  | {
      field: "limit"
      value: PolicyLimitPatch
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PolicyLimitKey".
 */
export type PolicyLimitKey =
  | "maxTaskSeconds"
  | "terminalSeconds"
  | "terminalIdleSeconds"
  | "transferSeconds"
  | "transferStallSeconds"
  | "concurrentTasks"
  | "concurrentTerminals"
  | "concurrentTransfers"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ResolvedLocale".
 */
export type ResolvedLocale = "en" | "zh-CN"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ResolvedTheme".
 */
export type ResolvedTheme = "light" | "dark"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "LocaleMode".
 */
export type LocaleMode = "system" | "zh-CN" | "en"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NotificationMode".
 */
export type NotificationMode = "off" | "unfocused" | "always"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ThemeMode".
 */
export type ThemeMode = "system" | "light" | "dark"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "BackendStatus".
 */
export type BackendStatus = "starting" | "ready" | "unavailable"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "SavePhase".
 */
export type SavePhase = "idle" | "saving" | "confirming" | "unknown"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ErrorCode".
 */
export type ErrorCode =
  | "invalid_request"
  | "conflict"
  | "storage_unavailable"
  | "protocol_mismatch"
  | "host_unavailable"
  | "timeout"
  | "cancelled"
  | "too_late"
  | "busy"
  | "shutting_down"
  | "revision_limit"
  | "runtime_unavailable"
  | "result_unknown"
  | "invalid_params"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileEditorRequest".
 */
export type FileEditorRequest =
  | {
      action: "list"
    }
  | {
      action: "show"
    }
  | {
      action: "chooseEditor"
    }
  | {
      action: "open"
      alternative: boolean
      path: string
      sessionId: string
    }
  | {
      action: "reopen"
      id: string
    }
  | {
      action: "retry"
      id: string
    }
  | {
      action: "recover"
      id: string
      sessionId: string
    }
  | {
      action: "resolve"
      choice: string
      destination?: string | null
      id: string
    }
  | {
      action: "export"
      id: string
    }
  | {
      action: "end"
      discard: boolean
      id: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RpcCommand".
 */
export type RpcCommand =
  | {
      method: "uiState.get"
      params: {
        scope: string
      }
    }
  | {
      method: "uiState.update"
      params: UiStateUpdate
    }
  | {
      method: "uiState.reconcile"
      params: {
        requestId: string
        runtimeId: string
        scope: string
      }
    }
  | {
      method: "runtime.hello"
      params: {
        application: string
        contractVersion: number
      }
    }
  | {
      method: "preferences.get"
      params: {}
    }
  | {
      method: "preferences.update"
      params: UpdatePreferencesRequest
    }
  | {
      method: "preferences.reconcile"
      params: {
        requestId: string
        runtimeId: string
      }
    }
  | {
      method: "requests.getStatus"
      params: {
        requestId: string
      }
    }
  | {
      method: "requests.cancel"
      params: {
        requestId: string
      }
    }
  | {
      method: "runtime.shutdown"
      params: {}
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "UiStateData".
 */
export type UiStateData =
  | {
      connectionRevision: number
      /**
       * @maxItems 64
       */
      expanded: string[]
      kind: "directory"
      path: string
      scroll: number
    }
  | {
      download: string
      kind: "pickers"
      upload: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RuntimeRequest".
 */
export type RuntimeRequest = {
  id: string
  jsonrpc: string
} & RuntimeRequest1
export type RuntimeRequest1 =
  | {
      method: "uiState.get"
      params: {
        scope: string
      }
    }
  | {
      method: "uiState.update"
      params: UiStateUpdate
    }
  | {
      method: "uiState.reconcile"
      params: {
        requestId: string
        runtimeId: string
        scope: string
      }
    }
  | {
      method: "runtime.hello"
      params: {
        application: string
        contractVersion: number
      }
    }
  | {
      method: "preferences.get"
      params: {}
    }
  | {
      method: "preferences.update"
      params: UpdatePreferencesRequest
    }
  | {
      method: "preferences.reconcile"
      params: {
        requestId: string
        runtimeId: string
      }
    }
  | {
      method: "requests.getStatus"
      params: {
        requestId: string
      }
    }
  | {
      method: "requests.cancel"
      params: {
        requestId: string
      }
    }
  | {
      method: "runtime.shutdown"
      params: {}
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RuntimeResponse".
 */
export type RuntimeResponse =
  | {
      id: string
      jsonrpc: "2.0"
      result: RpcValue
    }
  | {
      error: RpcError
      id: string | null
      jsonrpc: "2.0"
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RpcValue".
 */
export type RpcValue =
  | {
      kind: "uiState"
      state: UiStateRecord
    }
  | {
      kind: "uiReconciled"
      outcome: WriteOutcome | null
      state: UiStateRecord
    }
  | {
      application: string
      contractVersion: number
      kind: "ready"
      runtimeId: string
    }
  | {
      kind: "preferences"
      preferences: Preferences
    }
  | {
      kind: "updated"
      outcome: "applied" | "unchanged"
      preferences: Preferences
    }
  | {
      kind: "reconciled"
      outcome: WriteOutcome | null
      preferences: Preferences
    }
  | {
      kind: "requestStatus"
      outcome: WriteOutcome | null
      state: "queued" | "running" | "completed" | "unknown"
    }
  | {
      kind: "cancelled"
    }
  | {
      kind: "stopped"
    }
  | {
      kind: "workspace"
      result: WorkspaceReply
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceReply".
 */
export type WorkspaceReply =
  | {
      kind: "connectionAccess"
      state: ConnectionAccessState
    }
  | {
      edit: FileEditRemote
      kind: "fileEdit"
    }
  | {
      entry: RemoteEntry
      kind: "fileEntry"
    }
  | {
      kind: "fileOperation"
      page: FileOperationPage
    }
  | {
      kind: "agent"
      result: AgentReply
    }
  | {
      kind: "toolSnapshot"
      snapshot: ToolSnapshot
    }
  | {
      kind: "directory"
      page: DirectoryPage
    }
  | {
      grant: FileGrantView
      kind: "fileGrant"
    }
  | {
      kind: "transferItems"
      page: TransferItemsPage
    }
  | {
      kind: "transfers"
      /**
       * @maxItems 1000
       */
      tasks: TransferView[]
    }
  | {
      kind: "transferPage"
      next: string | null
      requestId: string
      /**
       * @maxItems 100
       */
      tasks: TransferView[]
    }
  | {
      kind: "identities"
      /**
       * @maxItems 64
       */
      names: IdentityName[]
    }
  | {
      kind: "proxies"
      next: number | null
      /**
       * @maxItems 50
       */
      proxies: ProxyView[]
      version: number
    }
  | {
      /**
       * @maxItems 50
       */
      connectionNames: string[]
      kind: "proxyReferences"
      next: number | null
      version: number
    }
  | {
      kind: "tunnels"
      /**
       * @maxItems 32
       */
      states: TunnelStatus[]
    }
  | {
      connectionAccess: ConnectionAccessState
      kind: "ready"
      version: 16
    }
  | {
      kind: "catalogue"
      page: CataloguePage
    }
  | {
      group: ConnectionGroupView
      kind: "group"
    }
  | {
      kind: "layout"
      revision: number
    }
  | {
      connection: ConnectionDetails
      kind: "connection"
      trust: TrustView | null
    }
  | {
      draft: CredentialDraftView
      kind: "draft"
    }
  | {
      kind: "operation"
      operation: WorkspaceOperation
    }
  | {
      kind: "sessions"
      /**
       * @maxItems 48
       */
      sessions: TerminalSessionView[]
    }
  | {
      kind: "done"
    }
  | {
      error: string | null
      kind: "reconciled"
      result: WorkspaceReply | null
    }
/**
 * Availability of the shared key, independent of IPC readiness.
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionAccessPhase".
 */
export type ConnectionAccessPhase = "locked" | "unlocking" | "ready" | "blocked"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentReply".
 */
export type AgentReply =
  | {
      kind: "workspaceTerminal"
      result: WorkspaceTerminalReply
    }
  | {
      kind: "activitySources"
      sources: ActivitySource[]
    }
  | {
      items: ActivitySummary[]
      kind: "activitySummaries"
      more: boolean
    }
  | {
      environment_known: boolean
      kind: "policyCapabilities"
      tools: ToolCapability[]
    }
  | {
      id: string
      kind: "policyBrowser"
    }
  | {
      kind: "policyResources"
      page: PolicyResourcePage
    }
  | {
      checkpoint?: ScreenCheckpoint | null
      kind: "screen"
    }
  | {
      kind: "capabilities"
      policy: ConnectionPolicy
      protections: SystemProtection[]
      support?: ExecutionEnvironment | null
      tools: ToolCapability[]
    }
  | {
      kind: "activity"
      page: ActivityPage
    }
  | {
      has_more?: boolean
      kind: "recordings"
      sessions: RecordedSession[]
    }
  | {
      kind: "input"
      request_key: string
      state: string
    }
  | {
      kind: "observations"
      observations: AccessObservation[]
    }
  | {
      endpoint_revision: number
      entry: AccessEntry
      kind: "authenticated"
    }
  | {
      code: string
      kind: "error"
    }
  | {
      kind: "done"
    }
  | {
      configuration: AgentConfiguration
      kind: "configuration"
      protections?: SystemProtection[]
      recording_error?: string | null
    }
  | {
      id: string
      kind: "challenge"
      revision: number
    }
  | {
      connections: AgentConnectionView[]
      kind: "connections"
    }
  | {
      context: AgentContext
      kind: "context"
    }
  | {
      contexts: AgentContext[]
      kind: "contexts"
    }
  | {
      kind: "task"
      task: AgentTask
    }
  | {
      kind: "tasks"
      tasks: AgentTask[]
    }
  | {
      kind: "output"
      output: TaskOutput
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceTerminalReply".
 */
export type WorkspaceTerminalReply =
  | {
      kind: "tabs"
      tabs: WorkspaceTerminalTab[]
    }
  | {
      kind: "control"
      tab: WorkspaceTerminalTab
    }
  | {
      cursor: number
      incomplete: boolean
      kind: "output"
      text: string
    }
  | {
      kind: "input"
      requestKey: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PolicyResourcePage".
 */
export type PolicyResourcePage = {
  kind: "directory"
  page: AgentFilesPage
  path: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AuthenticationMethod".
 */
export type AuthenticationMethod = "password" | "privateKey"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionEnvironment".
 */
export type ConnectionEnvironment =
  "none" | "development" | "test" | "staging" | "production"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "CredentialIntent".
 */
export type CredentialIntent = "keep" | "replace" | "remove"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "OperationPhase".
 */
export type OperationPhase =
  "working" | "trust" | "success" | "failed" | "cancelled"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TerminalEndKind".
 */
export type TerminalEndKind =
  | "userDisconnected"
  | "cancelled"
  | "remoteExit"
  | "remoteClosed"
  | "transportError"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TerminalPhase".
 */
export type TerminalPhase =
  | "connecting"
  | "trust"
  | "authenticating"
  | "opening"
  | "running"
  | "closed"
  | "failed"
  | "cancelled"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "SystemNotificationCommand".
 */
export type SystemNotificationCommand = "status" | "openSettings"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NotificationCategory".
 */
export type NotificationCategory = "attentionRequired" | "executionIssue"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NotificationTarget".
 */
export type NotificationTarget =
  | {
      batchId: string
      connectionId: string
      kind: "transfer"
      phase: TransferNotificationPhase
      runtimeId: string
    }
  | {
      contextId: string
      kind: "agentTask"
      review: boolean
      taskId: string
    }
  | {
      contextId: string
      kind: "agentContext"
    }
  | {
      contextId: string
      kind: "agentActivity"
    }
  | {
      id: string
      kind: "fileEditor"
    }
  | {
      kind: "fileOperation"
      operationId: string
      runtimeId: string
      sessionId: string
    }
  | {
      connectionId: string
      kind: "connection"
      runtimeId: string
      sessionId?: string | null
      tunnelId?: string | null
    }
  | {
      kind: "runtime"
    }
  | {
      kind: "mcp"
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TransferNotificationPhase".
 */
export type TransferNotificationPhase = "failed" | "awaitingDecision"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NotificationFeedbackDisposition".
 */
export type NotificationFeedbackDisposition =
  "pending" | "local" | "submitted" | "unknown" | "presented"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NotificationFeedbackRequest".
 */
export type NotificationFeedbackRequest =
  | {
      action: "list"
    }
  | {
      action: "presented"
      /**
       * @maxItems 100
       */
      ids: string[]
    }
  | {
      action: "activate"
      id: string
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NotificationOutcome".
 */
export type NotificationOutcome =
  | "submitted"
  | "suppressed"
  | "duplicate"
  | "failed"
  | "unknown"
  | "opened"
  | "stopped"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NotificationAuthorization".
 */
export type NotificationAuthorization =
  | "notDetermined"
  | "authorized"
  | "denied"
  | "provisional"
  | "unknown"
  | "unsupported"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceCommand".
 */
export type WorkspaceCommand =
  | {
      method: "files.edit"
      params: FileEditCommand
    }
  | {
      method: "agent.request"
      params: {
        request: AgentOperation
      }
    }
  | {
      method: "tools.subscribe"
      params: {
        groupId: string
        visible: boolean
      }
    }
  | {
      method: "tools.refresh"
      params: {
        sessionId: string
      }
    }
  | {
      method: "files.list"
      params: {
        cursor: string | null
        path: string
        requestId: string
        sessionId: string
      }
    }
  | {
      method: "files.releaseList"
      params: {
        requestId: string
      }
    }
  | {
      method: "files.identities"
      params: {
        /**
         * @maxItems 64
         */
        gids: number[]
        sessionId: string
        /**
         * @maxItems 64
         */
        uids: number[]
      }
    }
  | {
      method: "files.cancelList"
      params: {
        requestId: string
      }
    }
  | {
      method: "files.stat"
      params: {
        path: string
        sessionId: string
      }
    }
  | {
      method: "files.create"
      params: {
        path: string
        sessionId: string
      }
    }
  | {
      method: "files.startOperation"
      params: {
        clearMask: number
        operation: "remove" | "permissions"
        /**
         * @minItems 1
         * @maxItems 100
         */
        paths: string[]
        recursive: boolean
        sessionId: string
        setMask: number
      }
    }
  | {
      method: "files.operation"
      params: {
        id: string
        offset: number
      }
    }
  | {
      method: "files.retryOperation"
      params: {
        id: string
      }
    }
  | {
      method: "files.mkdir"
      params: {
        path: string
        sessionId: string
      }
    }
  | {
      method: "files.rename"
      params: {
        from: string
        sessionId: string
        to: string
      }
    }
  | {
      method: "files.remove"
      params: {
        directory: boolean
        path: string
        sessionId: string
      }
    }
  | {
      method: "files.pick"
      params: {
        direction: "upload" | "download"
      }
    }
  | {
      method: "files.grant"
      params: {
        direction: string
        paths: string[]
      }
    }
  | {
      method: "files.transfer"
      params: {
        grantId: string
        overwrite: boolean
        /**
         * @minItems 1
         * @maxItems 100
         */
        paths: string[]
        sessionId: string
      }
    }
  | {
      method: "files.archive"
      params: {
        grantId: string
        /**
         * @minItems 1
         * @maxItems 100
         */
        paths: string[]
        sessionId: string
      }
    }
  | {
      method: "transfers.items"
      params: {
        id: string
        offset: number
      }
    }
  | {
      method: "transfers.cancel"
      params: {
        id: string
      }
    }
  | {
      method: "transfers.retry"
      params: {
        id: string
      }
    }
  | {
      method: "transfers.list"
      params: {
        cursor?: string | null
        requestId?: string | null
      }
    }
  | {
      method: "transfers.resolveConflict"
      params: {
        batch: boolean
        decision: "skip" | "overwrite" | "cancel"
        id: string
        version: number
      }
    }
  | {
      method: "proxies.list"
      params: {
        offset: number
        version: number | null
      }
    }
  | {
      method: "proxies.references"
      params: {
        id: string
        offset: number
        version: number | null
      }
    }
  | {
      method: "proxies.save"
      params: ProxySave
    }
  | {
      method: "proxies.delete"
      params: VersionedId
    }
  | {
      method: "proxyCredentialDrafts.begin"
      params: {
        proxyId: string | null
      }
    }
  | {
      method: "tunnels.list"
      params: {
        connectionId: string
      }
    }
  | {
      method: "tunnels.retry"
      params: {
        connectionId: string
        ruleId: string
      }
    }
  | {
      method: "terminals.validateInput"
      params: {
        sessionId: string
        text: string
      }
    }
  | {
      method: "groups.updateLayout"
      params: GroupLayout
    }
  | {
      method: "workspace.negotiate"
      params: {
        version: 16
      }
    }
  | {
      method: "workspace.context"
      params: {
        locale: "en" | "zh-CN"
        theme: "light" | "dark"
      }
    }
  | {
      method: "workspace.detach"
      params: {}
    }
  | {
      method: "connections.unlock"
      params: {}
    }
  | {
      method: "connections.initialize"
      params: {}
    }
  | {
      method: "catalogue.list"
      params: CatalogueQuery
    }
  | {
      method: "groups.create"
      params: {
        name: string
      }
    }
  | {
      method: "groups.update"
      params: {
        id: string
        name: string
        version: number
      }
    }
  | {
      method: "groups.delete"
      params: VersionedId
    }
  | {
      method: "connections.create"
      params: ConnectionSave
    }
  | {
      method: "connections.update"
      params: ConnectionSave
    }
  | {
      method: "connections.get"
      params: {
        id: string
      }
    }
  | {
      method: "connections.delete"
      params: {
        disconnectSessions: boolean
        id: string
        /**
         * @maxItems 32
         */
        sessionIds: string[]
        version: number
      }
    }
  | {
      method: "credentials.reconcile"
      params: {
        id: string
      }
    }
  | {
      method: "credentialDrafts.begin"
      params: {
        connectionId: string | null
      }
    }
  | {
      method: "credentialDrafts.configure"
      params: {
        auth: AuthenticationMethod
        draftId: string
      }
    }
  | {
      method: "credentialDrafts.clear"
      params: {
        draftId: string
      }
    }
  | {
      method: "credentialDrafts.discard"
      params: {
        draftId: string
      }
    }
  | {
      method: "connections.test"
      params: ConnectionTest
    }
  | {
      method: "hostTrust.confirm"
      params: {
        challengeId: string
      }
    }
  | {
      method: "hostTrust.reject"
      params: {
        challengeId: string
      }
    }
  | {
      method: "hostTrust.forget"
      params: VersionedId
    }
  | {
      method: "terminals.open"
      params: {
        columns: number
        connectionId: string
        rows: number
      }
    }
  | {
      method: "terminals.list"
      params: {}
    }
  | {
      method: "terminals.resize"
      params: {
        columns: number
        rows: number
        sessionId: string
      }
    }
  | {
      method: "terminals.close"
      params: {
        sessionId: string
      }
    }
  | {
      method: "operations.get"
      params: {
        id: string
      }
    }
  | {
      method: "operations.cancel"
      params: {
        id: string
      }
    }
  | {
      method: "connections.reconcile"
      params: {
        requestId: string
        runtimeId: string
      }
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileEditCommand".
 */
export type FileEditCommand =
  | {
      action: "open"
      id: string
      leaseOwner: string
      local: string
      path: string
      sessionId: string
    }
  | {
      action: "inspect"
      id: string
    }
  | {
      action: "upload"
      destination?: string | null
      expected: RemoteFileVersion
      id: string
      local: string
    }
  | {
      action: "reload"
      id: string
      local: string
    }
  | {
      action: "close"
      id: string
    }
/**
 * 只用于受信 Main 与 Backend 的私有 IPC，不能通过通用 Renderer 工作区入口转发。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentOperation".
 */
export type AgentOperation =
  | {
      operation: "activitySources"
    }
  | {
      filter: ActivitySummaryFilter
      operation: "activitySummaries"
    }
  | {
      operation: "rejectScript"
      task_id: string
    }
  | {
      operation: "previewPolicy"
      policy: ConnectionPolicy
    }
  | {
      connection_id: string
      connection_revision: number
      operation: "openPolicyBrowser"
    }
  | {
      browser_id: string
      operation: "readPolicyResources"
      query: PolicyResourceQuery
    }
  | {
      browser_id: string
      operation: "closePolicyBrowser"
    }
  | {
      checkpoint: ScreenCheckpoint
      operation: "saveScreen"
      task_id: string
    }
  | {
      operation: "loadScreen"
      task_id: string
    }
  | {
      operation: "retryRecording"
    }
  | {
      cursor: number
      include_output?: boolean | null
      operation: "activity"
      session_id: string
      task_id?: string | null
    }
  | {
      filter: RecordingFilter
      operation: "recordings"
    }
  | {
      operation: "closeSession"
      session_id: string
    }
  | {
      code: string
      digest: string
      operation: "reject"
      tool: string
    }
  | {
      digest: string
      operation: "authenticate"
    }
  | {
      operation: "configuration"
    }
  | {
      operation: "observations"
    }
  | {
      endpoint_revision: number
      entry_id: string
      generation: number
      operation: "observe"
      port: number
    }
  | {
      change: AgentMutation
      operation: "prepare"
    }
  | {
      challenge_id: string
      operation: "commit"
    }
  | {
      challenge_id: string
      operation: "abort"
    }
  | {
      call: AgentCall
      digest: string
      operation: "invoke"
    }
  | {
      operation: "contexts"
    }
  | {
      operation: "task"
      task_id: string
    }
  | {
      connection_id?: string | null
      entry_id?: string | null
      offset: number
      operation: "tasks"
      phase?: string | null
      session_id?: string | null
    }
  | {
      cursor: number
      operation: "output"
      task_id: string
    }
  | {
      operation: "cancel"
      task_id: string
    }
  | {
      operation: "stop"
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentMutation".
 */
export type AgentMutation =
  | {
      kind: "approveScript"
      task_id: string
    }
  | {
      kind: "bootstrap"
      secret: AccessSecret
      settings: AgentSettings
    }
  | {
      kind: "settings"
      settings: AgentSettings
    }
  | {
      kind: "settingsPatch"
      patch: AgentSettingsPatch
    }
  | {
      delivery: AgentDelivery
      kind: "provision"
      port: number
      secret: AccessSecret
    }
  | {
      client: AgentClient
      delivery: AgentDelivery
      generation: number
      id: string
      kind: "export"
      port: number
    }
  | {
      connection_id: string
      connection_revision: number
      expected_revision: number
      kind: "updatePolicySettings"
      patch: PolicySettingsPatch
    }
  | {
      connection_id: string
      connection_revision: number
      enabled: boolean
      expected_revision: number
      kind: "setPolicyAccess"
    }
  | {
      kind: "access"
      secret: AccessSecret
    }
  | {
      id: string
      kind: "revoke"
    }
  | {
      generation: number
      id: string
      kind: "deleteAccess"
    }
  | {
      context_id: string
      kind: "approveConnection"
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "CatalogueKind".
 */
export type CatalogueKind = "groups" | "connections"
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceEvent".
 */
export type WorkspaceEvent =
  | {
      kind: "connectionAccess"
      state: ConnectionAccessState
    }
  | {
      kind: "toolSnapshot"
      snapshot: ToolSnapshot
    }
  | {
      kind: "transfer"
      task: TransferView
    }
  | {
      code: string
      kind: "inputRejected"
      sessionId: string
    }
  | {
      connectionId: string
      kind: "tunnels"
      /**
       * @maxItems 32
       */
      states: TunnelStatus[]
    }
  | {
      kind: "catalogue"
    }
  | {
      kind: "operation"
      operation: WorkspaceOperation
    }
  | {
      kind: "session"
      session: TerminalSessionView
    }
  | {
      kind: "detached"
    }
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceRequest".
 */
export type WorkspaceRequest = {
  id: string
  jsonrpc: string
  owner: string
} & WorkspaceRequest1
export type WorkspaceRequest1 =
  | {
      method: "files.edit"
      params: FileEditCommand
    }
  | {
      method: "agent.request"
      params: {
        request: AgentOperation
      }
    }
  | {
      method: "tools.subscribe"
      params: {
        groupId: string
        visible: boolean
      }
    }
  | {
      method: "tools.refresh"
      params: {
        sessionId: string
      }
    }
  | {
      method: "files.list"
      params: {
        cursor: string | null
        path: string
        requestId: string
        sessionId: string
      }
    }
  | {
      method: "files.releaseList"
      params: {
        requestId: string
      }
    }
  | {
      method: "files.identities"
      params: {
        /**
         * @maxItems 64
         */
        gids: number[]
        sessionId: string
        /**
         * @maxItems 64
         */
        uids: number[]
      }
    }
  | {
      method: "files.cancelList"
      params: {
        requestId: string
      }
    }
  | {
      method: "files.stat"
      params: {
        path: string
        sessionId: string
      }
    }
  | {
      method: "files.create"
      params: {
        path: string
        sessionId: string
      }
    }
  | {
      method: "files.startOperation"
      params: {
        clearMask: number
        operation: "remove" | "permissions"
        /**
         * @minItems 1
         * @maxItems 100
         */
        paths: string[]
        recursive: boolean
        sessionId: string
        setMask: number
      }
    }
  | {
      method: "files.operation"
      params: {
        id: string
        offset: number
      }
    }
  | {
      method: "files.retryOperation"
      params: {
        id: string
      }
    }
  | {
      method: "files.mkdir"
      params: {
        path: string
        sessionId: string
      }
    }
  | {
      method: "files.rename"
      params: {
        from: string
        sessionId: string
        to: string
      }
    }
  | {
      method: "files.remove"
      params: {
        directory: boolean
        path: string
        sessionId: string
      }
    }
  | {
      method: "files.pick"
      params: {
        direction: "upload" | "download"
      }
    }
  | {
      method: "files.grant"
      params: {
        direction: string
        paths: string[]
      }
    }
  | {
      method: "files.transfer"
      params: {
        grantId: string
        overwrite: boolean
        /**
         * @minItems 1
         * @maxItems 100
         */
        paths: string[]
        sessionId: string
      }
    }
  | {
      method: "files.archive"
      params: {
        grantId: string
        /**
         * @minItems 1
         * @maxItems 100
         */
        paths: string[]
        sessionId: string
      }
    }
  | {
      method: "transfers.items"
      params: {
        id: string
        offset: number
      }
    }
  | {
      method: "transfers.cancel"
      params: {
        id: string
      }
    }
  | {
      method: "transfers.retry"
      params: {
        id: string
      }
    }
  | {
      method: "transfers.list"
      params: {
        cursor?: string | null
        requestId?: string | null
      }
    }
  | {
      method: "transfers.resolveConflict"
      params: {
        batch: boolean
        decision: "skip" | "overwrite" | "cancel"
        id: string
        version: number
      }
    }
  | {
      method: "proxies.list"
      params: {
        offset: number
        version: number | null
      }
    }
  | {
      method: "proxies.references"
      params: {
        id: string
        offset: number
        version: number | null
      }
    }
  | {
      method: "proxies.save"
      params: ProxySave
    }
  | {
      method: "proxies.delete"
      params: VersionedId
    }
  | {
      method: "proxyCredentialDrafts.begin"
      params: {
        proxyId: string | null
      }
    }
  | {
      method: "tunnels.list"
      params: {
        connectionId: string
      }
    }
  | {
      method: "tunnels.retry"
      params: {
        connectionId: string
        ruleId: string
      }
    }
  | {
      method: "terminals.validateInput"
      params: {
        sessionId: string
        text: string
      }
    }
  | {
      method: "groups.updateLayout"
      params: GroupLayout
    }
  | {
      method: "workspace.negotiate"
      params: {
        version: 16
      }
    }
  | {
      method: "workspace.context"
      params: {
        locale: "en" | "zh-CN"
        theme: "light" | "dark"
      }
    }
  | {
      method: "workspace.detach"
      params: {}
    }
  | {
      method: "connections.unlock"
      params: {}
    }
  | {
      method: "connections.initialize"
      params: {}
    }
  | {
      method: "catalogue.list"
      params: CatalogueQuery
    }
  | {
      method: "groups.create"
      params: {
        name: string
      }
    }
  | {
      method: "groups.update"
      params: {
        id: string
        name: string
        version: number
      }
    }
  | {
      method: "groups.delete"
      params: VersionedId
    }
  | {
      method: "connections.create"
      params: ConnectionSave
    }
  | {
      method: "connections.update"
      params: ConnectionSave
    }
  | {
      method: "connections.get"
      params: {
        id: string
      }
    }
  | {
      method: "connections.delete"
      params: {
        disconnectSessions: boolean
        id: string
        /**
         * @maxItems 32
         */
        sessionIds: string[]
        version: number
      }
    }
  | {
      method: "credentials.reconcile"
      params: {
        id: string
      }
    }
  | {
      method: "credentialDrafts.begin"
      params: {
        connectionId: string | null
      }
    }
  | {
      method: "credentialDrafts.configure"
      params: {
        auth: AuthenticationMethod
        draftId: string
      }
    }
  | {
      method: "credentialDrafts.clear"
      params: {
        draftId: string
      }
    }
  | {
      method: "credentialDrafts.discard"
      params: {
        draftId: string
      }
    }
  | {
      method: "connections.test"
      params: ConnectionTest
    }
  | {
      method: "hostTrust.confirm"
      params: {
        challengeId: string
      }
    }
  | {
      method: "hostTrust.reject"
      params: {
        challengeId: string
      }
    }
  | {
      method: "hostTrust.forget"
      params: VersionedId
    }
  | {
      method: "terminals.open"
      params: {
        columns: number
        connectionId: string
        rows: number
      }
    }
  | {
      method: "terminals.list"
      params: {}
    }
  | {
      method: "terminals.resize"
      params: {
        columns: number
        rows: number
        sessionId: string
      }
    }
  | {
      method: "terminals.close"
      params: {
        sessionId: string
      }
    }
  | {
      method: "operations.get"
      params: {
        id: string
      }
    }
  | {
      method: "operations.cancel"
      params: {
        id: string
      }
    }
  | {
      method: "connections.reconcile"
      params: {
        requestId: string
        runtimeId: string
      }
    }

export interface DesktopContract {
  agent: AgentContract
  desktop: DesktopSnapshot
  error_code: ErrorCode
  file_editors: FileEditorContract
  runtime: RuntimeContract
  system_notifications: SystemNotificationContract
  workspace: WorkspaceContract
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentContract".
 */
export interface AgentContract {
  call: AgentCall
  event: AgentEvent
  execution: ExecutionContract
  snapshot: AgentSnapshot
  ui_request: AgentUiRequest
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TextReplacement".
 */
export interface TextReplacement {
  after: string
  before: string
}
/**
 * 本地传输引用已登记目录；远端内部仅使用固定账号标识，MCP 远端输入为绝对路径。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScopedPath".
 */
export interface ScopedPath {
  path: string
  scopeId: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ServiceTarget".
 */
export interface ServiceTarget {
  manager: ServiceManager
  name: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NativeServiceDefinition".
 */
export interface NativeServiceDefinition {
  content: ServiceDefinitionContent
  expectedHash?: string | null
  target: ServiceTarget
}
/**
 * 省略字段保留原值；更新来源不支持修改名称或格式，也不自动延续信任。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PackageSourcePatch".
 */
export interface PackageSourcePatch {
  components?: string[] | null
  public_key?: string | null
  suite?: string | null
  url?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "DnsConfiguration".
 */
export interface DnsConfiguration {
  searchDomains: string[]
  servers: string[]
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ActivityRecord".
 */
export interface ActivityRecord {
  actor: ActivityActor
  body: ActivityBody
  parentTaskId?: string | null
  policyRevision: number
  sequence: number
  sessionId: string
  taskId?: string | null
  timestampMs: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScriptOrigin".
 */
export interface ScriptOrigin {
  /**
   * 仅在已记录的完整内容摘要与执行快照一致时成立，不从相同路径推断。
   */
  contentMatches: boolean
  sessionId: string
  taskId: string
  tool: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScriptSnapshot".
 */
export interface ScriptSnapshot {
  arguments: string[]
  directory: string
  hash: string
  interpreter: ScriptInterpreter
  path: string
  /**
   * 仅保留既有记录的读取；当前执行不生成或复核文件属性版本。
   */
  sourceVersion?: string | null
  text: string
}
/**
 * Main 的派生终端画面缓存；丢失后仍可从完整输入输出恢复，不代表执行状态。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScreenCheckpoint".
 */
export interface ScreenCheckpoint {
  columns: number
  gzip: string
  rows: number
  sequence: number
  version: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionPolicy".
 */
export interface ExecutionPolicy {
  downloadRoots: LocalDirectoryGrant[]
  fileTransfer: boolean
  level: OperationLevel
  limits: ExecutionLimits
  protectSystem: boolean
  protectedPaths: ProtectedPath[]
  scriptReview?: boolean
  uploadRoots: LocalDirectoryGrant[]
  version: 2
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "LocalDirectoryGrant".
 */
export interface LocalDirectoryGrant {
  id: string
  path: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionLimits".
 */
export interface ExecutionLimits {
  concurrentTasks: number
  concurrentTerminals: number
  concurrentTransfers: number
  maxTaskSeconds?: number | null
  terminalIdleSeconds?: number | null
  terminalSeconds?: number | null
  transferSeconds?: number | null
  transferStallSeconds?: number | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ProtectedPath".
 */
export interface ProtectedPath {
  kind: ScopeKind
  path: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "OperationTarget".
 */
export interface OperationTarget {
  effect: FileEffect
  path: string
  recursive: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentTask".
 */
export interface AgentTask {
  command: string
  connectionId: string
  connectionName: string
  connectionRevision?: number
  contextId: string
  createdAt: number
  cwd: string
  entryId: string
  entryName?: string
  error?: string | null
  executionState?: ExecutionState | null
  exitCode?: number | null
  fileEffects?: FileCommitState | null
  finishedAt?: number | null
  historical?: HistoricalTask | null
  id: string
  operation?: RuntimeOperation | null
  outputEnd: number
  outputStart: number
  parentTaskId?: string | null
  phase: TaskPhase
  policyRevision: number
  /**
   * 业务执行的简化授权快照；仅查询或尚未接纳的审计记录可以没有快照。
   */
  policySnapshot?: ExecutionPolicy | null
  recoveryPaths?: string[]
  result?: string | null
  revision?: number
  scriptSnapshot?: ScriptSnapshot | null
  targets: OperationTarget[]
  terminal: boolean
  terminationConfirmed: boolean
  tool: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionState".
 */
export interface ExecutionState {
  dispatch: DispatchState
  outcome?: ExecutionOutcome | null
  phase: ExecutionPhase
  review: ScriptReviewState
  terminal: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ScriptReviewBinding".
 */
export interface ScriptReviewBinding {
  identity: ExecutionIdentity
  snapshot: ScriptSnapshot
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionIdentity".
 */
export interface ExecutionIdentity {
  connectionId: string
  connectionRevision: number
  entryGeneration: number
  entryId: string
  policyRevision: number
  sessionId: string
  taskId: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "HistoricalTask".
 */
export interface HistoricalTask {
  active: boolean
  connectionId: string
  createdAt: number
  display: HistoricalTaskDisplay
  entryId: string
  fingerprint?: string | null
  id: string
  /**
   * SQLite 保存的原正文；展示解析不重编码或修补这段内容。
   */
  rawBody: string
  requestKey?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "HistoricalTaskDisplay".
 */
export interface HistoricalTaskDisplay {
  command?: string | null
  connectionName?: string | null
  contextId?: string | null
  cwd?: string | null
  entryName?: string | null
  error?: string | null
  exitCode?: number | null
  finishedAt?: number | null
  outputEnd?: number | null
  outputStart?: number | null
  phase?: string | null
  tool?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentContext".
 */
export interface AgentContext {
  connectionId: string
  connectionName: string
  connectionRevision?: number
  createdAt: number
  cwd: string
  entryId: string
  entryName: string
  environment: {
    [k: string]: string
  }
  error?: string | null
  id: string
  phase: ContextPhase
  revision: number
  support?: ExecutionEnvironment | null
  terminal?: WorkspaceTerminalTab | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionEnvironment".
 */
export interface ExecutionEnvironment {
  accountUid?: number | null
  architecture: string
  atomicRename: boolean
  operatingSystem: string
  packageManagers: PackageManagerSupport[]
  programs: string[]
  reason?: string | null
  sftp: boolean
  supportedTools: string[]
  systemServiceManager: boolean
  userServiceManager: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PackageManagerSupport".
 */
export interface PackageManagerSupport {
  available: boolean
  kind: PackageManagerKind
  reason?: string | null
  version?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceTerminalTab".
 */
export interface WorkspaceTerminalTab {
  connectionId: string
  control?: WorkspaceAgentControl | null
  id: string
  name: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceAgentControl".
 */
export interface WorkspaceAgentControl {
  contextId: string
  entryId: string
  entryName: string
  leaseId: string
  releasing: boolean
}
/**
 * 当前执行与只读历史合同，与管理请求和 MCP 输入共同生成。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionContract".
 */
export interface ExecutionContract {
  authorization: ExecutionAuthorization
  outcome: ExecutionOutcome
  policy: ConnectionPolicy
  request: ExecutionRequest
  state: ExecutionState
  task_history: TaskHistoryPage
  task_history_filter: TaskHistoryFilter
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionAuthorization".
 */
export interface ExecutionAuthorization {
  commandCheck: CommandCheckRequirement
  scriptReviewRequired: boolean
  /**
   * 明确文件目标仍须在适配器中核实实际路径、链接、版本和提交能力。
   */
  targetCheckRequired: boolean
  targets: OperationTarget[]
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionPolicy".
 */
export interface ConnectionPolicy {
  connectionId: string
  connectionRevision: number
  execution: ExecutionPolicy
  revision: number
  verifyConnect: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ExecutionRequest".
 */
export interface ExecutionRequest {
  identity: ExecutionIdentity
  operation: RuntimeOperation
  requestKey: string
  timeoutSeconds?: number | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TaskHistoryPage".
 */
export interface TaskHistoryPage {
  nextOffset?: number | null
  tasks: HistoricalTask[]
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TaskHistoryFilter".
 */
export interface TaskHistoryFilter {
  connectionId?: string | null
  entryId?: string | null
  offset: number
  phase?: string | null
  sessionId?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentSnapshot".
 */
export interface AgentSnapshot {
  clients: AgentClientDetection[]
  contexts: AgentContext[]
  endpoint: string
  entries: AccessEntry[]
  error?: string | null
  observations: AccessObservation[]
  policies: ConnectionPolicySettings[]
  protections?: SystemProtection[]
  running: boolean
  settings: AgentSettings
  views: AgentSessionView[]
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentClientDetection".
 */
export interface AgentClientDetection {
  client: AgentClient
  configurationFound: boolean
  installed: boolean
  unavailable: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AccessEntry".
 */
export interface AccessEntry {
  client?: AgentClient | null
  enabled: boolean
  generation: number
  id: string
  name: string
}
/**
 * 记录成功接入历史；地址版本使端口改回旧值时仍需重新验证。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AccessObservation".
 */
export interface AccessObservation {
  endpoint: string
  endpointRevision?: number
  entryId: string
  firstAt: number
  generation: number
  lastAt: number
}
/**
 * Live configuration only. Historical task/recording policy snapshots keep their existing shape.
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionPolicySettings".
 */
export interface ConnectionPolicySettings {
  accessEnabled: boolean
  connectionId: string
  connectionRevision: number
  execution: ExecutionPolicy
  revision: number
  verifyConnect: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "SystemProtection".
 */
export interface SystemProtection {
  category: ProtectionCategory
  /**
   * Directory 包含全部后代；File 只保护该对象，不能推导为保护整个父目录。
   */
  kind: ScopeKind
  maintenance: string
  path: string
  reason: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentSettings".
 */
export interface AgentSettings {
  enabled: boolean
  endpointRevision?: number
  port: number
  retentionDays: number
  revision: number
  showTray: boolean
  storageMib: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentSessionView".
 */
export interface AgentSessionView {
  context: AgentContext
  runningTasks: number
}
/**
 * Read-only cross-session activity projection. Cursor ordering is timestamp then identity.
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ActivitySummaryFilter".
 */
export interface ActivitySummaryFilter {
  active?: boolean | null
  before?: string | null
  beforeTime?: number | null
  query?: string | null
  sessionId?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RecordingFilter".
 */
export interface RecordingFilter {
  active?: boolean | null
  beforeCreatedAt?: number | null
  beforeId?: string | null
  connectionId?: string | null
  createdAfter?: number | null
  entryId?: string | null
  offset: number
  query?: string | null
  result?: string | null
  sessionId?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PolicyLimitPatch".
 */
export interface PolicyLimitPatch {
  key: PolicyLimitKey
  value?: number | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "DesktopSnapshot".
 */
export interface DesktopSnapshot {
  appearance: Appearance
  error: string | null
  preferences: Preferences | null
  runtime: RuntimeStatus
  save: SaveState
  version: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "Appearance".
 */
export interface Appearance {
  locale: ResolvedLocale
  reducedTransparency: boolean
  theme: ResolvedTheme
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "Preferences".
 */
export interface Preferences {
  attentionRequiredNotificationEnabled: boolean
  connectionOpenBehavior: "ask" | "reuse" | "new"
  downloadDirectory: string
  downloadMode: "ask" | "directory"
  executionIssueNotificationEnabled: boolean
  fileEditorApplication: string
  filesAutoOpen: boolean
  /**
   * @minItems 1
   * @maxItems 6
   */
  filesColumns: string[]
  filesShowHidden: boolean
  filesShowTree: boolean
  localeMode: LocaleMode & ("system" | "en" | "zh-CN")
  monitorDisplay: "bar" | "chart"
  monitorFullSeconds: 60 | 120
  monitorSampleSeconds: 1 | 5 | 15
  remoteDirectoryMode: "last" | "initial"
  revision: number
  sidebarHoverPreview: boolean
  systemNotificationMode: NotificationMode
  terminalCopyOnSelect: boolean
  terminalCursorBlink: boolean
  terminalCursorStyle: "block" | "bar" | "underline"
  terminalFontFamily:
    | "jetbrains-mono"
    | "menlo"
    | "monaco"
    | "consolas"
    | "cascadia-mono"
    | "cascadia-code"
    | "dejavu-sans-mono"
    | "liberation-mono"
    | "courier-new"
  terminalFontSize: number
  terminalLineHeight: number
  terminalScrollback: 1000 | 5000 | 10000 | 20000 | 50000
  terminalThemeMode: "inherit" | "light" | "dark"
  themeMode: ThemeMode
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RuntimeStatus".
 */
export interface RuntimeStatus {
  instanceId: string | null
  status: BackendStatus
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "SaveState".
 */
export interface SaveState {
  error: string | null
  phase: SavePhase
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileEditorContract".
 */
export interface FileEditorContract {
  reply: FileEditorReply
  request: FileEditorRequest
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileEditorReply".
 */
export interface FileEditorReply {
  error?: string | null
  ok: boolean
  sessions: FileEditorView[]
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileEditorView".
 */
export interface FileEditorView {
  application: string
  connectionId: string
  error?: string | null
  id: string
  path: string
  phase: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RuntimeContract".
 */
export interface RuntimeContract {
  command: RpcCommand
  notification: RuntimeNotification
  patch: PreferencesPatch
  preferences: Preferences
  request: RuntimeRequest
  response: RuntimeResponse
  update: UpdatePreferencesRequest
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "UiStateUpdate".
 */
export interface UiStateUpdate {
  data: UiStateData
  expectedRevision: number
  scope: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "UpdatePreferencesRequest".
 */
export interface UpdatePreferencesRequest {
  base: PreferencesPatch
  expectedRevision: number
  patch: PreferencesPatch
}
/**
 * 字段缺省表示保持原值；显式 null 不是合法偏好。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "PreferencesPatch".
 */
export interface PreferencesPatch {
  attentionRequiredNotificationEnabled?: boolean
  connectionOpenBehavior?: "ask" | "reuse" | "new"
  downloadDirectory?: string
  downloadMode?: "ask" | "directory"
  executionIssueNotificationEnabled?: boolean
  fileEditorApplication?: string
  filesAutoOpen?: boolean
  /**
   * @minItems 1
   * @maxItems 6
   */
  filesColumns?: string[]
  filesShowHidden?: boolean
  filesShowTree?: boolean
  localeMode?: LocaleMode & ("system" | "en" | "zh-CN")
  monitorDisplay?: "bar" | "chart"
  monitorFullSeconds?: 60 | 120
  monitorSampleSeconds?: 1 | 5 | 15
  remoteDirectoryMode?: "last" | "initial"
  sidebarHoverPreview?: boolean
  systemNotificationMode?: NotificationMode
  terminalCopyOnSelect?: boolean
  terminalCursorBlink?: boolean
  terminalCursorStyle?: "block" | "bar" | "underline"
  terminalFontFamily?:
    | "jetbrains-mono"
    | "menlo"
    | "monaco"
    | "consolas"
    | "cascadia-mono"
    | "cascadia-code"
    | "dejavu-sans-mono"
    | "liberation-mono"
    | "courier-new"
  terminalFontSize?: number
  terminalLineHeight?: number
  terminalScrollback?: 1000 | 5000 | 10000 | 20000 | 50000
  terminalThemeMode?: "inherit" | "light" | "dark"
  themeMode?: ThemeMode
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RuntimeNotification".
 */
export interface RuntimeNotification {
  jsonrpc: "2.0"
  method: "preferences.changed"
  params: ChangedParams
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ChangedParams".
 */
export interface ChangedParams {
  preferences: Preferences
  runtimeId: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "UiStateRecord".
 */
export interface UiStateRecord {
  data: UiStateData | null
  revision: number
  scope: string
}
/**
 * 仅作为近期写入的确认凭证；不用于持久任务或崩溃恢复。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WriteOutcome".
 */
export interface WriteOutcome {
  error?: RpcError | null
  preferences?: Preferences | null
  status: "applied" | "unchanged" | "rejected"
  uiState?: UiStateRecord | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RpcError".
 */
export interface RpcError {
  code: number
  data: ErrorData
  message: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ErrorData".
 */
export interface ErrorData {
  code: string
  preferences?: Preferences | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionAccessState".
 */
export interface ConnectionAccessState {
  code?: string | null
  phase: ConnectionAccessPhase
  revision: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileEditRemote".
 */
export interface FileEditRemote {
  connectionId: string
  groupId: string
  id: string
  path: string
  version: RemoteFileVersion
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RemoteFileVersion".
 */
export interface RemoteFileVersion {
  gid: number
  hash: string
  modified: number
  permissions: number
  size: number
  uid: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RemoteEntry".
 */
export interface RemoteEntry {
  gid: number | null
  kind: "file" | "directory" | "symlink" | "other"
  modified: number | null
  name: string
  path: string
  permissions: number | null
  size: number | null
  uid: number | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileOperationPage".
 */
export interface FileOperationPage {
  completed: number
  error?: string | null
  failed: number
  id: string
  /**
   * @maxItems 100
   */
  items: FileOperationItem[]
  next?: number | null
  pending: number
  phase: "working" | "success" | "failed" | "cancelled"
  skipped: number
  stage: "scanning" | "applying" | "finished"
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileOperationItem".
 */
export interface FileOperationItem {
  entry: RemoteEntry
  error?: string | null
  status: "pending" | "complete" | "failed" | "skipped"
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ActivitySource".
 */
export interface ActivitySource {
  client?: AgentClient | null
  id: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ActivitySummary".
 */
export interface ActivitySummary {
  connectionName: string
  createdAt: number
  entryId?: string
  entryName: string
  finishedAt?: number | null
  id: string
  kind: string
  phase?: TaskPhase | null
  sequence?: number | null
  sessionId: string
  startedAt?: number | null
  summary: string
  taskId?: string | null
  uncertain: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ToolCapability".
 */
export interface ToolCapability {
  configured: boolean
  reason?: string | null
  /**
   * 目标路径、系统账号权限与资源变更必须在执行时复核，不作预先成功保证。
   */
  requiresExecutionCheck: boolean
  supported: boolean
  tool: string
  usable: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentFilesPage".
 */
export interface AgentFilesPage {
  entries: AgentFileEntry[]
  nextCursor?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentFileEntry".
 */
export interface AgentFileEntry {
  gid: number
  kind: string
  mode: number
  modifiedSeconds?: number | null
  name: string
  path: string
  size: number
  uid: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ActivityPage".
 */
export interface ActivityPage {
  incomplete: boolean
  nextCursor: number
  records: ActivityRecord[]
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "RecordedSession".
 */
export interface RecordedSession {
  bytes: number
  connectionId: string
  connectionName: string
  createdAt: number
  endedAt?: number | null
  entryId: string
  entryName: string
  id: string
  incomplete: boolean
  lastSequence: number
  result?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentConfiguration".
 */
export interface AgentConfiguration {
  entries: AccessSecret[]
  policies: ConnectionPolicySettings[]
  settings: AgentSettings
}
/**
 * 加密令牌只通过 Main/Backend 私有通道传递，不属于 Renderer 快照。
 *
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AccessSecret".
 */
export interface AccessSecret {
  digest: string
  entry: AccessEntry
  sealed: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "AgentConnectionView".
 */
export interface AgentConnectionView {
  environment: string
  id: string
  name: string
  policy: ConnectionPolicy
  tabs: WorkspaceTerminalTab[]
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TaskOutput".
 */
export interface TaskOutput {
  chunks: OutputChunk[]
  incomplete: boolean
  nextCursor: number
  screen?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "OutputChunk".
 */
export interface OutputChunk {
  sequence: number
  stream: string
  text: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ToolSnapshot".
 */
export interface ToolSnapshot {
  connectionId: string
  groupId: string
  sample: ServerSample
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ServerSample".
 */
export interface ServerSample {
  architecture: string | null
  collectedAt: number
  cpuPercent: number | null
  /**
   * @maxItems 10000
   */
  disks: DiskUsage[]
  distribution: string | null
  error: string | null
  hostname: string | null
  kernel: string | null
  memoryAvailable: number | null
  memoryTotal: number | null
  networkReceive: number | null
  networkSend: number | null
  phase: "loading" | "ready" | "stale" | "unavailable"
  uptime: number | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "DiskUsage".
 */
export interface DiskUsage {
  available: number
  mount: string
  total: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "DirectoryPage".
 */
export interface DirectoryPage {
  /**
   * @maxItems 100
   */
  entries: RemoteEntry[]
  next: string | null
  path: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileGrantView".
 */
export interface FileGrantView {
  id: string
  /**
   * @maxItems 100
   */
  names: string[]
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TransferItemsPage".
 */
export interface TransferItemsPage {
  /**
   * @maxItems 100
   */
  items: TransferItem[]
  next: number | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TransferItem".
 */
export interface TransferItem {
  error: string | null
  path: string
  phase: "pending" | "complete" | "failed" | "skipped" | "conflict"
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TransferView".
 */
export interface TransferView {
  attempt: number
  batchId: string
  batchSize: number
  bytes: number
  canRetry: boolean
  conflict: TransferConflict | null
  conflictVersion: number
  connectionId: string
  createdAt: number
  details?: TransferSummary | null
  direction: "upload" | "download"
  error: string | null
  groupId: string
  id: string
  name: string
  phase:
    | "queued"
    | "running"
    | "cancelling"
    | "awaitingDecision"
    | "complete"
    | "cancelled"
    | "failed"
    | "skipped"
  remoteIdentity: string
  remotePath: string
  revision: number
  total: number | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TransferConflict".
 */
export interface TransferConflict {
  sourcePath: string | null
  targetPath: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TransferSummary".
 */
export interface TransferSummary {
  completed: number
  failed: number
  pending: number
  skipped: number
  stage: "enumerating" | "transferring" | "archiving" | "verifying" | "finished"
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "IdentityName".
 */
export interface IdentityName {
  group: boolean
  id: number
  name: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ProxyView".
 */
export interface ProxyView {
  config: ProxyConfig
  credentialSaved: boolean
  id: string
  version: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ProxyConfig".
 */
export interface ProxyConfig {
  host: string
  kind: "socks5" | "http"
  name: string
  port: number
  username: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TunnelStatus".
 */
export interface TunnelStatus {
  error: string | null
  phase: "stopped" | "disabled" | "running" | "failed"
  ruleId: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "CataloguePage".
 */
export interface CataloguePage {
  /**
   * @maxItems 50
   */
  connections: ConnectionSummary[]
  /**
   * @maxItems 50
   */
  groups: ConnectionGroupView[]
  layoutRevision: number
  next: number | null
  version: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionSummary".
 */
export interface ConnectionSummary {
  auth: AuthenticationMethod
  createdAt: string | null
  credentialPending: boolean
  credentialSaved: boolean
  deleting: boolean
  environment: ConnectionEnvironment
  groupId: string | null
  groupSortOrder: number
  host: string
  id: string
  name: string
  revision: number
  version: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionGroupView".
 */
export interface ConnectionGroupView {
  id: string
  name: string
  sortOrder: number
  version: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionDetails".
 */
export interface ConnectionDetails {
  config: ConnectionConfig
  createdAt: string | null
  credentialLabel: string | null
  credentialPending: boolean
  credentialSaved: boolean
  deleting: boolean
  groupId: string | null
  groupSortOrder: number
  id: string
  revision: number
  version: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionConfig".
 */
export interface ConnectionConfig {
  auth: AuthenticationMethod
  environment?: ConnectionEnvironment
  host: string
  name: string
  notes?: string
  port: number
  proxyId?: string | null
  terminal?: TerminalSettings
  /**
   * @maxItems 32
   */
  tunnels?: TunnelRule[]
  username: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TerminalSettings".
 */
export interface TerminalSettings {
  backspace: "del" | "bs"
  delete: "vt220" | "del"
  encoding: "utf-8" | "gb18030" | "gbk" | "big5"
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TunnelRule".
 */
export interface TunnelRule {
  bindHost: string
  enabled: boolean
  id: string
  kind: "local" | "remote" | "socks5"
  listenPort: number
  name: string
  targetHost: string | null
  targetPort: number | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TrustView".
 */
export interface TrustView {
  algorithm: string
  confirmedAt: string
  fingerprint: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "CredentialDraftView".
 */
export interface CredentialDraftView {
  auth: (AuthenticationMethod | null) & ("password" | "privateKey" | null)
  connectionId: string | null
  id: string
  intent: CredentialIntent
  label: string | null
  prepared: boolean
  proxyId: string | null
  saved: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceOperation".
 */
export interface WorkspaceOperation {
  challenge: TrustChallenge | null
  connection: ConnectionDetails | null
  draft: CredentialDraftView | null
  error: string | null
  fileSummary?: FileOperationSummary | null
  id: string
  kind: string
  phase: OperationPhase
  proxy: ProxyView | null
  sessionId: string | null
  stage: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TrustChallenge".
 */
export interface TrustChallenge {
  algorithm: string
  expiresInSeconds: number
  fingerprint: string
  host: string
  id: string
  name: string
  port: number
  previousFingerprint: string | null
  temporary: boolean
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "FileOperationSummary".
 */
export interface FileOperationSummary {
  attempt: number
  completed: number
  connectionId: string
  elapsedMs: number
  failed: number
  skipped: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TerminalSessionView".
 */
export interface TerminalSessionView {
  agentControl?: WorkspaceAgentControl | null
  connectionId: string
  connectionRevision: number
  endReason?: TerminalEndReason | null
  error: string | null
  exitCode: number | null
  groupId: string
  host: string
  id: string
  name: string
  operationId: string
  phase: TerminalPhase
  port: number
  terminal: TerminalSettings
  username: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TerminalEndReason".
 */
export interface TerminalEndReason {
  kind: TerminalEndKind
  message?: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "SystemNotificationContract".
 */
export interface SystemNotificationContract {
  command: SystemNotificationCommand
  feedback: NotificationFeedback
  feedback_request: NotificationFeedbackRequest
  result: SystemNotificationResult
  status: SystemNotificationStatus
  transfer_feedback: TransferFeedbackReport
  transfer_target: TransferNotificationTarget
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "NotificationFeedback".
 */
export interface NotificationFeedback {
  body: string
  category: NotificationCategory
  id: string
  relatedTargets: NotificationTarget[]
  relevant: boolean
  route: NotificationFeedbackDisposition
  target: NotificationTarget
  title: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "SystemNotificationResult".
 */
export interface SystemNotificationResult {
  outcome: NotificationOutcome
  status: SystemNotificationStatus
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "SystemNotificationStatus".
 */
export interface SystemNotificationStatus {
  authorization: NotificationAuthorization
  version: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TransferFeedbackReport".
 */
export interface TransferFeedbackReport {
  /**
   * @maxItems 100
   */
  keys: string[]
  runtimeId: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "TransferNotificationTarget".
 */
export interface TransferNotificationTarget {
  batchId: string
  connectionId: string
  phase: TransferNotificationPhase
  runtimeId: string
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceContract".
 */
export interface WorkspaceContract {
  command: WorkspaceCommand
  connection_access_snapshot: ConnectionAccessSnapshot
  notification: WorkspaceNotification
  reply: WorkspaceReply
  request: WorkspaceRequest
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ProxySave".
 */
export interface ProxySave {
  clearPassword: boolean
  config: ProxyConfig
  draftId: string | null
  expectedVersion: number | null
  proxyId: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "VersionedId".
 */
export interface VersionedId {
  id: string
  version: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "GroupLayout".
 */
export interface GroupLayout {
  buckets: GroupBucket[]
  groupIds: string[]
  revision: number
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "GroupBucket".
 */
export interface GroupBucket {
  connectionIds: string[]
  groupId: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "CatalogueQuery".
 */
export interface CatalogueQuery {
  kind: CatalogueKind
  offset: number
  query: string
  version: number | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionSave".
 */
export interface ConnectionSave {
  config: ConnectionConfig
  connectionId: string | null
  draftId: string | null
  expectedVersion: number | null
  groupId: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionTest".
 */
export interface ConnectionTest {
  config: ConnectionConfig | null
  connectionId: string | null
  draftId: string | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "ConnectionAccessSnapshot".
 */
export interface ConnectionAccessSnapshot {
  runtimeId?: string | null
  state?: ConnectionAccessState | null
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceNotification".
 */
export interface WorkspaceNotification {
  jsonrpc: "2.0"
  method: "workspace.changed"
  params: WorkspaceNotificationParams
}
/**
 * This interface was referenced by `DesktopContract`'s JSON-Schema
 * via the `definition` "WorkspaceNotificationParams".
 */
export interface WorkspaceNotificationParams {
  event: WorkspaceEvent
  owner: string
  runtimeId: string
}
