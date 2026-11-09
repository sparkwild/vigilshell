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
| `build` | 使用 release 身份编译 Rust、原生桥及 production 前端 |
| `package:dev` / `package:release` | 打包 Dev 应用／正式 DMG |

## 签名准备

macOS 使用独立的 Dev／release 应用身份，当前为固定证书自签名、无公证，`hardenedRuntime: false`。配置采用完整 SHA-1 证书指纹及钥匙串绝对路径：

```sh
security find-identity -v -p codesigning
npm run signing:configure -- --profile dev --identity <40位指纹> --keychain /绝对路径/login.keychain-db
npm run signing:check -- --profile dev
```

正式打包使用 `--profile release`。配置位于 `~/Library/Application Support/VigilShell Signing/`；证书与私钥留在仓库外，已有配置沿用当前身份。实现见[签名配置](../scripts/signing/config.mjs)与[打包脚本](../scripts/package-macos.mjs)。

本地热更新沿用现有 Dev 身份、`out/`、`target/debug/` 和 `.cache/{notification-native,mcp-native}/`。打包构建通过统一上下文把前端、原生桥、后端资源和 Cargo 输出放到 `.cache/build/<dev|release>/<arm64|x64>/` 的 `out/`、`native/`、`backend/`、`cargo/`，不覆盖热更新产物。`package:dev` 使用 Dev 签名和 `development-package` 模式，`package:release` 使用 release 签名和 `production` 模式；构建模式与身份不匹配时失败。原生桥在编译阶段即采用所选身份，不要求 release 构建配置 Dev 证书。

打包入口移除继承的开发页面、启动器及数据目录环境变量；本地已有 Dev 应用运行时仍拒绝完整重建。安装包内的 `out/`、`Resources/native/` 和 `Helpers/` 布局、Dev／正式应用数据目录保持独立。最终产物仍输出到 `release/dev/` 或 `release/`。

## 检查与预览构建

Node、npm、Rust 分别读取 `.nvmrc`、`package.json`、`rust-toolchain.toml`。项目通过本地命令执行检查、签名和打包：`npm run check` 运行静态检查，`npm run backend:check` 运行后端检查，`npm run test:signing` 验证签名与构建逻辑。`npm run release:check` 校验应用、npm 锁文件、Rust workspace 和 Cargo 锁文件中的本项目版本。源码与发行版本统一使用 `X.Y.Z`；发行版本准备使用 `npm run version:prepare -- X.Y.Z` 同步四份版本文件，不创建标签。

`npm run package:release -- --arch arm64` 使用本地 release 签名配置构建 Apple Silicon 安装包；打包脚本同时保留 x64 能力。ARM64 安装包命名为 `VigilShell-<version>-macos-arm64.dmg`，同时生成 SHA-256 和包含版本、源码 SHA、证书身份的签名报告，打包入口始终使用 `--publish never`。

GitHub CI 对面向 main 的 PR 和 main 推送运行前后端检查；原生、后端、依赖、打包与 CI 改动调用 macOS ARM64 打包及启动检查。固定汇总检查名为 `CI verified`，仅在所需任务成功时通过。Actions 默认只读，不读取固定签名 secrets。

`Preview package` 支持手动运行并选择完整提交 SHA，也供 CI 复用。预览使用一次性自签名证书及临时钥匙串，清理成功后才收集安装包。产物包含 DMG、SHA-256、签名报告和 `build.json`，记录源码提交、Git tree、版本、架构、工具链及运行信息，保存 14 天。预览只用于隔离测试；源码提交须包含对应构建工具。当前工作流不处理版本标签、不创建或发布 Release。贡献与版本操作见 [CONTRIBUTING.md](../CONTRIBUTING.md)。

自签名不等于 Apple 公证；当前提供版本发现与发布页跳转，下载安装由用户手动完成。

## 更新与许可

Main 在用户手动检查时通过公开 GitHub Releases 列表匿名查询标签为 `vX.Y.Z` 且含兼容安装包的正式发行版本，过滤草稿和 GitHub 标记为预发布的 Release；自动检查由共享功能开关停用，启动、周期与睡眠恢复调度实现保留。更新不下载或替换应用。关于页提供实际包版本、项目链接与版权信息；许可文件随安装包分发。

自有代码采用 Apache-2.0。`npm run licenses:generate` 根据锁定依赖和上游补充生成第三方声明；已声明许可但未附正文的依赖保留包声明与来源，仅提示，不阻止打包，也不补写未知版权人。`npm run licenses:check` 核对清单与许可资源；未声明许可、清单过期、资源缺失或内容不一致仍会失败。正式打包要求许可检查通过，并逐字节核对包内资源。
