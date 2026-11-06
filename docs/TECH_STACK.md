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

## 持续集成

Ubuntu 的 `frontend.yml` 和 `backend.yml` 在 `main`／`develop` 推送及 PR 中运行，也通过 `workflow_call` 为签名工作流提供同一源码 SHA 的前置检查。工作流并发组包含调用工作流名称，避免独立检查与复用检查互相取消。

macOS CI 仅构建 ARM64（`macos-15`）。PR 使用临时证书验证 production 应用；`macos-signing.yml` 的手动入口允许本仓库 `develop` 或 `main`，只上传 Actions 产物。`vX.Y.Z-beta.N` 标签触发发布，标签必须匹配应用版本且属于远程 `main` 历史。底层 x64 打包能力保留，但不在当前 CI 矩阵中。

Node、npm、Rust 分别读取 `.nvmrc`、`package.json`、`rust-toolchain.toml`。`npm run release:check` 校验应用、npm 锁文件、Rust workspace 和 Cargo 锁文件中的本项目版本。当前 Beta 版本是 `0.1.0-beta.1`。

固定签名通过 `python3 scripts/configure-release-signing.py` 配置。独立 RSA 3072／SHA-256 自签名证书有效期 10 年，加密 P12、证书和指纹备份保存在仓库外的 `~/Library/Application Support/VigilShell Signing/release-backup/`，密码保存在独立钥匙串密码条目中。配置不会导入本机代码签名证书或修改 Dev 身份。重跑复用原备份，不更换证书。

配置优先使用 `macos-release` 环境，只有 GitHub 明确返回套餐限制才回退仓库级；网络、权限和签名错误均报错。所选作用域包含 `MACOS_SIGNING_P12_BASE64`、`MACOS_SIGNING_P12_PASSWORD` Secrets 和 `MACOS_SIGNING_IDENTITY` 变量，仓库变量 `MACOS_SIGNING_STORAGE` 记录 `environment` 或 `repository`。两个互斥 Job 使用同一 composite action，仓库模式不声明 GitHub Environment。环境允许 `main`、`develop` 与 `v*-beta.*` 标签；签名脚本另外校验事件、精确版本和源码。固定凭据不会提供给 PR。

CI 仅接受显式注入的 release 签名身份与临时钥匙串路径，不回读本机配置。两种存储方式都使用相同的固定证书和 production 打包路径。临时钥匙串、导出的私钥和 P12 均由 runner 清理，恢复钥匙串搜索列表；清理失败阻止可分发产物上传。

`npm run test:ci-macos` 只在 GitHub 托管 macOS runner 的全新数据目录运行，验证签名报告、应用身份、窗口、Renderer IPC、真实 Rust 后端及正常退出，拒绝已有 Dev 或正式数据。诊断附件保留 7 天；启动探针不代替真实 SSH、钥匙串交互或首次下载安装验证。

安装包命名为 `VigilShell-<version>-macos-arm64.dmg`，同时提供 SHA-256 和包含版本、源码 SHA、证书身份的签名报告。标签发布在检查、打包、启动和清理成功后创建草稿，上传并回读核对资产哈希，再设为 Prerelease，且不标记 Latest。仅允许恢复同标签、同 SHA 的草稿，不覆盖已发布版本或移动标签。手动验证永远不会创建 Release。自签名不等于 Apple 公证；当前提供版本发现与发布页跳转，下载安装由用户手动完成。

## 更新与许可

Main 在用户手动检查时通过公开 GitHub Releases 列表匿名查询兼容的稳定／Beta 发行包；自动检查由共享功能开关停用，启动、周期与睡眠恢复调度实现保留。更新不下载或替换应用。关于页提供实际包版本、项目链接与版权信息；许可文件随安装包分发。

自有代码采用 Apache-2.0。`npm run licenses:generate` 根据锁定依赖和上游补充生成第三方声明；已声明许可但未附正文的依赖保留包声明与来源，仅提示，不阻止打包，也不补写未知版权人。`npm run licenses:check` 核对清单与许可资源；未声明许可、清单过期、资源缺失或内容不一致仍会失败。正式打包要求许可检查通过，并逐字节核对包内资源。
