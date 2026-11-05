# 技术栈

精确版本以 [package.json](../package.json)、[package-lock.json](../package-lock.json)、[Cargo.toml](../Cargo.toml)、[Cargo.lock](../Cargo.lock)和[rust-toolchain.toml](../rust-toolchain.toml)为准。升级直接更新当前选型及对应配置、锁文件。

## 环境与依赖

| 层面 | 当前选择 |
| --- | --- |
| 开发环境 | Node.js 24.21.0、npm 11.19.1、Rust 1.96.0／Edition 2024；macOS Xcode Command Line Tools |
| 桌面与构建 | Electron 44、electron-vite 5、Vite 7、electron-builder |
| 界面 | React 19、TypeScript 6、Tailwind CSS 4 |
| 组件 | shadcn/ui Base UI，base-nova／neutral；React Aria 树与表格 |
| 图标与字体 | Lucide；本地品牌 SVG；Fontsource Inter、JetBrains Mono、Noto Sans SC |
| 图表与动效 | shadcn Chart、Recharts 3、AutoAnimate |
| 表单 | React Hook Form 7、Zod 4；Zod 用于表单校验 |
| 终端 | xterm.js、FitAddon、SearchAddon；Headless／Serialize 用于终端数据处理 |
| 国际化 | Paraglide，zh-CN／en，消息源位于 `messages/` |
| Rust 运行时 | Tokio、tokio-util、tracing |
| 存储 | SQLite、rusqlite bundled、rusqlite_migration |
| SSH/SFTP | russh、russh-sftp；encoding_rs 处理终端编码 |
| 凭据 | XChaCha20-Poly1305、macOS Keychain、zeroize |
| 通信类型 | Rust DTO → Schemars JSON Schema → TypeScript／Ajv；Serde 序列化 |
| MCP | Model Context Protocol TypeScript SDK，本机 HTTP 服务 |
| 测试与检查 | Vitest、React Testing Library、Rust tests、ESLint、Prettier、rustfmt、Clippy |

字体、图标、文案随应用分发，不依赖运行时 CDN。基础依赖约束见 [foundation-dependencies.json](../scripts/foundation-dependencies.json)，Rust 来源及许可证约束见 [deny.toml](../deny.toml)。组件使用方式见 [UI 组件与设计规范](UI_DESIGN.md)。

Tokio 固定为 `1.53.3`；加密依赖的锁文件包含 `crypto-bigint 0.7.10` 与 `ctutils 0.4.8`。`cap-std` 的 Windows 间接依赖 `winx 0.36.4` 使用 `Apache-2.0 WITH LLVM-exception`，许可证许可仅限定该包和版本，其他依赖继续按通用清单检查。

## 工具入口

使用 `npm run <命令>`，脚本定义见 [package.json](../package.json)。

| 命令 | 用途 |
| --- | --- |
| `dev:app` | 签名原生 Dev 与热更新 |
| `check` | 依赖、边界、lint、格式、合同、国际化、类型与 Rust 静态检查 |
| `contracts:generate` / `contracts:check` | 生成／核对通信类型与 Schema |
| `i18n:generate` / `i18n:check` | 生成本地化代码／核对消息资源 |
| `build` | 编译 Rust、原生桥及前端 |
| `package:dev` / `package:release` | 打包 Dev 应用／正式 DMG |

## 签名准备

macOS 使用独立的 Dev／release 应用身份，当前为固定证书自签名、无公证，`hardenedRuntime: false`。配置采用完整 SHA-1 证书指纹及钥匙串绝对路径：

```sh
security find-identity -v -p codesigning
npm run signing:configure -- --profile dev --identity <40位指纹> --keychain /绝对路径/login.keychain-db
npm run signing:check -- --profile dev
```

正式打包使用 `--profile release`。配置位于 `~/Library/Application Support/VigilShell Signing/`；证书与私钥留在仓库外，已有配置沿用当前身份。实现见[签名配置](../scripts/signing/config.mjs)与[打包脚本](../scripts/package-macos.mjs)。

## 持续集成

Ubuntu 工作流保留前后端静态检查、测试和依赖审计。`macos-ci.yml` 在 PR、`main`／`develop` 推送及手动触发时，分别使用 `macos-15`（ARM64）和 `macos-15-intel`（x64）进行原生构建与测试。

macOS CI 运行完整 Rust 检查、前端类型与测试、签名逻辑和开发会话测试，再构建签名的 Dev 应用。任务临时生成代码签名证书，保存在独立钥匙串中，结束时移除；不读取 release 签名 Secrets，也不生成 Release。

`npm run test:ci-macos` 仅允许 GitHub 托管的 macOS runner 执行，并拒绝已有 Dev 或正式应用数据目录。它使用 runner 上一次性的 Dev 数据，检查打包应用的窗口、Renderer IPC、真实 Rust 后端进程和正常退出，完成后清理数据。截图、启动日志和签名报告作为保留 7 天的 Actions 诊断附件。该检查不执行真实 SSH 操作，也不代替钥匙串交互、Touch ID 或首次下载安装的人工验收。
