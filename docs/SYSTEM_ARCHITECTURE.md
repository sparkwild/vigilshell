# 系统架构

## 进程与通信

应用由 Electron Main、Preload、React Renderer 和 Rust Backend 组成。

- **Renderer**：界面、导航、表单和展示状态，通过具名 DesktopApi 发起操作。
- **Preload**：受控桥接，不向页面暴露任意系统调用。
- **Main**：窗口、IPC 来源校验、Backend 子进程、MCP HTTP、通知和 macOS 原生集成。
- **Backend**：业务用例、连接与任务生命周期、持久化及远端执行。

调用链为 Renderer → Preload → Main → Backend。Main 与 Backend 使用 JSON-RPC；终端数据使用 MessagePort 和独立二进制通道。通信结构由 Rust DTO 生成 Schema、TypeScript 和 Ajv 校验器，不维护手写副本。

## 前端目录

| 目录 | 职责 |
| --- | --- |
| `src/main/` | 桌面进程及平台集成 |
| `src/preload/` | DesktopApi 桥接 |
| `src/contracts/` | 桌面 API、生成合同与运行时校验 |
| `src/shared/` | 不依赖 Electron／React 的共享逻辑和国际化 |
| `src/renderer/app/` | 应用组装、导航和工作区布局 |
| `src/renderer/features/` | 按连接、终端、文件、设置、MCP 等功能组织业务 |
| `src/renderer/components/ui/` | 通用 UI 组件，不持有业务状态 |
| `src/renderer/styles/` | 全局样式与设计令牌 |
| `resources/`、`messages/` | 随包资源与消息源 |
| `scripts/`、`tests/` | 构建检查工具及测试 |

入口只负责组合；业务功能内按组件、模型和服务职责拆分。公共组件不依赖具体连接、任务或会话对象。

## Rust 分层

| crate | 职责 |
| --- | --- |
| `vigilshell-domain` | 值对象、不变量与纯业务规则 |
| `vigilshell-application-contracts` | 应用端口、输入、结果与事件；依赖 Domain |
| `vigilshell-application` | 用例与生命周期编排；依赖 Contracts 和 Domain |
| `vigilshell-desktop-protocol` | DTO、序列化与 Schema；依赖 Contracts 和 Domain |
| `vigilshell-backend` | CLI、依赖组装、报文调度与进程收尾 |
| `vigilshell-storage-sqlite` | 数据库、事务、请求回执与活动存储 |
| `vigilshell-credentials` | 凭据加密、系统密钥和原生输入 |
| `vigilshell-transport-russh` | SSH、PTY、代理、隧道、SFTP 与远端适配 |
| `vigilshell-local-files` | 本地授权目录句柄与文件保护 |

Domain 不依赖基础设施。应用服务通过 Contracts 调用适配器；适配器依赖 Contracts／Domain，不反向依赖应用实现。Backend 是依赖组装入口。技术内部端口留在所属组件，应用端口统一定义于 Contracts。

## 状态归属

| 状态 | 所有者 |
| --- | --- |
| 连接配置、偏好、策略、会话、任务和持久记录 | Rust 应用服务及存储适配器 |
| 工作区展示、导航与异步操作反馈 | WorkspaceStore／OperationStore |
| 主题、语言与原生窗口上下文 | Rust 保存偏好，Main 派生并同步 |
| 系统通知与文件编辑协调 | Main 服务；Renderer Store 提供界面快照 |
| MCP 接入与身份验证 | Main |
| Agent 会话与任务执行 | AgentService／LocalExecutor |
| 普通终端与控制租约 | WorkspaceService |

组件消费单一状态源，不因重复挂载建立第二份业务状态。通知、编辑和传输反馈订阅由应用级生命周期管理，不依赖具体页面。

## 资源生命周期

运行组固定连接配置及凭据引用，终端、工具、传输和外部编辑通过租约共享资源。最后一个租约结束后释放底层连接，界面卸载只停止展示。

xterm 实例与 React 挂载解耦，切换标签或隐藏工作区不重建 SSH shell。Agent 操作复用普通终端与文件界面，控制权归工作区服务；活动记录作为独立查询数据，不承担界面会话生命周期。

Backend 使用 Tokio 管理异步任务，SQLite 与系统密钥访问由独立线程处理。密码库协调器归 Backend 进程所有，连接配置和凭据共享解锁能力；密钥及凭据正文不进入 Renderer。

普通窗口关闭隐藏应用；显式退出统一收尾。Renderer 重载不重启 Backend，Backend 重启不恢复 SSH 会话。外部编辑工作副本由 Main 独立管理，终端关闭不直接销毁编辑任务。
