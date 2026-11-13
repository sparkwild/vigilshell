# 技术栈

精确版本以 [package.json](../package.json)、[package-lock.json](../package-lock.json)、[Cargo.toml](../Cargo.toml)、[Cargo.lock](../Cargo.lock)和[rust-toolchain.toml](../rust-toolchain.toml)为准。升级直接更新当前选型及对应配置、锁文件。

## 环境与依赖

| 层面 | 当前选择 |
| --- | --- |
| 开发环境 | Node.js 24.21.0、npm 11.19.1、Rust 1.96.0／Edition 2024；macOS Xcode Command Line Tools |
| 桌面与构建 | Electron 44、electron-vite 5、Vite 7、electron-builder、electron-updater 6.8.12 |
| 界面 | React 19、TypeScript 6、Tailwind CSS 4 |
| 组件 | shadcn/ui Base UI，base-nova／neutral；React Aria 树与表格 |
| 图标与字体 | Lucide；本地品牌 SVG；Fontsource Inter、JetBrains Mono、Noto Sans SC |
| 图表与动效 | shadcn Chart、Recharts 3、AutoAnimate |
| 表单 | React Hook Form 7、Zod 4；Zod 用于表单校验 |
| 终端 | xterm.js、FitAddon、SearchAddon；Headless／Serialize 用于终端数据处理 |
| 国际化 | Paraglide，zh-CN／en，消息源位于 `messages/` |
| Rust 运行时 | Tokio、tokio-util、tracing |
| 存储 | SQLite 3.54.1、rusqlite 0.41.1 bundled、rusqlite_migration |
| SSH/SFTP | russh、russh-sftp；encoding_rs 处理终端编码 |
| 凭据 | XChaCha20-Poly1305、macOS Keychain、zeroize |
| 通信类型 | Rust DTO → Schemars JSON Schema → TypeScript／Ajv；Serde 序列化 |
| MCP | Model Context Protocol TypeScript SDK，本机 HTTP 服务 |
| 测试与检查 | Vitest、React Testing Library、Rust tests、ESLint、Prettier、rustfmt、Clippy |

字体、图标、文案随应用分发，不依赖运行时 CDN。基础依赖约束见 [foundation-dependencies.json](../scripts/foundation-dependencies.json)，Rust 来源及许可证约束见 [deny.toml](../deny.toml)。组件使用方式见 [UI 组件与设计规范](UI_DESIGN.md)。

`rusqlite_migration` 固定使用上游 [rusqlite 0.41 兼容提交](https://github.com/cljoly/rusqlite_migration/commit/30b941a0850df790091dc58c3ec44f18dbb5e3ba)。

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

macOS 使用独立的 Dev／release 应用身份，当前为固定证书自签名、无公证，`hardenedRuntime: false`。开发者自行创建或导入代码签名证书及私钥，并选择证书导出文件的保管位置。将示例中的占位参数替换为完整的 40 位 SHA-1 证书指纹和证书所在钥匙串的绝对路径：

```sh
security find-identity -v -p codesigning
npm run signing:configure -- --profile dev \
  --identity "<DEV_CERTIFICATE_SHA1>" \
  --keychain "<DEV_KEYCHAIN_ABSOLUTE_PATH>"
npm run signing:check -- --profile dev
```

正式打包使用 `--profile release` 单独配置对应的证书指纹和钥匙串路径。配置脚本按当前用户将配置分别保存到 `dev`、`release` 目录，实际签名从所选钥匙串读取私钥；证书导出文件及私钥不存入源码仓库。同一发行线的后续版本保持签名身份连续，官方发行使用维护者管理的固定正式身份。实现见[签名配置](../scripts/signing/config.mjs)与[打包脚本](../scripts/package-macos.mjs)。

本地热更新沿用现有 Dev 身份、`out/`、`target/debug/` 和 `.cache/{notification-native,mcp-native}/`。打包构建通过统一上下文把前端、原生桥、后端资源和 Cargo 输出放到 `.cache/build/<dev|release>/<arm64|x64>/` 的 `out/`、`native/`、`backend/`、`cargo/`，不覆盖热更新产物。`package:dev` 使用 Dev 签名和 `development-package` 模式，`package:release` 使用 release 签名和 `production` 模式；构建模式与身份不匹配时失败。原生桥在编译阶段即采用所选身份，不要求 release 构建配置 Dev 证书。

打包入口移除继承的开发页面、启动器及数据目录环境变量；本地已有 Dev 应用运行时仍拒绝完整重建。安装包内的 `out/`、`Resources/native/` 和 `Helpers/` 布局、Dev／正式应用数据目录保持独立。最终产物仍输出到 `release/dev/` 或 `release/`。

仅供 Renderer 使用、由 Vite 合入前端产物的 npm 依赖放在 `devDependencies`；Main 中外置加载的依赖保留在 `dependencies`，由 electron-builder 收集其运行时依赖树。打包排除依赖包的 sourcemap、类型声明、测试、示例、基准和开发说明；保留运行时代码、条件导出的源码与许可证。`Resources/icons` 只复制主进程使用的应用图和托盘图，其余图标由构建工具或 Renderer 按需处理。`npm run test:packaging` 验证过滤规则与 Renderer 依赖的许可覆盖。

## 检查与预览构建

Node、npm、Rust 分别读取 `.nvmrc`、`package.json`、`rust-toolchain.toml`。项目通过本地命令执行检查、签名和打包：`npm run check` 运行静态检查，`npm run backend:check` 运行后端检查，`npm run test:signing` 验证签名与构建逻辑。`npm run release:check` 校验应用、npm 锁文件、Rust workspace 和 Cargo 锁文件中的本项目版本，`npm run release:check -- --notes` 同时检查发布说明与累计变更日志。源码与发行版本统一使用 `X.Y.Z`；发行版本准备使用 `npm run version:prepare -- X.Y.Z` 同步四份版本文件，不创建标签。

`npm run package:release -- --arch arm64` 使用本地 release 签名配置构建 Apple Silicon 安装包；打包脚本同时保留 x64 能力。产物包括 `VigilShell-<version>-macos-arm64.dmg`、同名更新 ZIP、两者的 SHA-256、ZIP blockmap、`latest-mac.yml` 和签名报告。更新元数据仅引用最终 ZIP，并核对版本、架构、大小和 SHA-512；DMG 的摘要在签名后生成。签名前规范化暂存包的资源写权限，保留执行权限和符号链接，避免 Squirrel 无法处理只读资源。打包入口始终使用 `--publish never`。

GitHub CI 对面向 `main` 的 PR 按路径分类。纯文档改动只运行分类和 `CI verified` 汇总，其他改动运行前后端检查。合并前要求分支跟上 `main`，并通过 `CI verified`。手动运行 CI 会对所选 ref 执行完整前后端检查。常规 CI 使用只读权限，完整安装包由手动预览和正式版本标签工作流构建。

`Preview package` 通过手动运行构建 macOS ARM64 安装包并验证启动，可指定完整提交 SHA。预览使用一次性自签名证书及临时钥匙串。上传前删除包含证书和私钥的临时钥匙串及签名临时文件，并验证钥匙串搜索列表已恢复。管理员信任记录随一次性 GitHub-hosted macOS 虚拟机销毁。清理成功后才收集安装包。产物包含 DMG、更新 ZIP、校验文件、blockmap、更新元数据、签名报告和 `build.json`，记录源码提交、Git tree、版本、架构、工具链及运行信息，保存 14 天。预览只用于隔离测试，源码提交须包含对应构建工具。贡献与版本操作见 [CONTRIBUTING.md](../CONTRIBUTING.md)。

## 正式版本草稿

`Release draft` 由新建的 `vX.Y.Z` 标签触发。资格检查核对版本文件、标签对应的 `docs/releases/vX.Y.Z.md`、生成的 `CHANGELOG.md`、远端标签对象及提交是否属于 `main` 历史。工作流使用资格检查确认的 SHA 执行完整前后端检查，两项都通过后才进入同一提交的 macOS 打包。固定签名仅在受限的 `macos-release` 环境中导入。该环境使用 `MACOS_SIGNING_P12_BASE64`、`MACOS_SIGNING_P12_PASSWORD` 两个 secret，以及记录证书指纹的 `MACOS_SIGNING_IDENTITY` 变量。

macOS ARM64 runner 验证 DMG 和包内签名，执行启动与退出检查，并在上传前清理私钥和临时钥匙串。DMG、ZIP、校验文件、blockmap、更新元数据、签名报告与 `build.json` 直接上传至 Release 草稿。创建草稿前重新核对最终 ZIP 与元数据摘要。工作流核对远端附件的摘要后按系统生成彩色下载表格，表格下方链接到累计变更日志。发布说明的编写和审阅规则见 [CONTRIBUTING.md](../CONTRIBUTING.md#release-notes)。正式发布由维护者在 GitHub 上执行，更新检查读取已发布的正式版本。

## 更新与许可

Main 在用户手动检查时通过公开 GitHub Releases 列表匿名查询标签为 `vX.Y.Z` 且含兼容安装包的正式发行版本，过滤草稿和 GitHub 标记为预发布的 Release；自动检查由共享功能开关停用，启动、周期与睡眠恢复调度实现保留。Main 使用 electron-updater 手动下载 ZIP，用户明确选择安装后才进行原生准备与重启替换；自动下载和普通退出安装均关闭。已下载包按摘要保存为不可变文件，回执原子替换，重启后校验版本、架构、大小和 SHA-512，支持离线恢复与安装。原生准备超时属于结果未知，保存安装回执后受控重启；重启仍为旧版本时展示安装失败，不自动重试。安装前验证目录写权限、解压空间及保存结果。关于页提供实际包版本、项目链接与版权信息；许可文件随安装包分发。

自有代码采用 Apache-2.0。`npm run licenses:generate` 根据锁定依赖和上游补充生成第三方声明；已声明许可但未附正文的依赖保留包声明与来源，仅提示，不阻止打包，也不补写未知版权人。`npm run licenses:check` 核对清单与许可资源；未声明许可、清单过期、资源缺失或内容不一致仍会失败。正式打包要求许可检查通过，并逐字节核对包内资源。

npm 许可证收集以运行时 `dependencies` 和 [Renderer 打包依赖清单](../scripts/bundled-dependencies.json) 的 `packages` 为入口，按锁文件追踪普通、可选和 peer 依赖，包含嵌套安装版本。`stylesheets` 记录只导入 CSS 的包，收集其许可证，不引入未分发的 CLI 依赖树。新增 Renderer 依赖时同步该清单；移入 `devDependencies` 不会免除已分发代码、CSS 和字体的许可证收集。

打包从锁定版本的 Electron 发行目录取完整 `LICENSES.chromium.html`，与应用并列放在 DMG 和 ZIP 根目录。打包校验挂载 DMG、解压 ZIP 并逐字节比对原文，同时检查 `.app` 内的许可布局、权限和签名。目录形式的 Dev 包也在 `.app` 旁提供同一份文件。

本地原生升级验收使用 `npm run test:update:native`，复用现有 Dev 身份和 Dev 数据；运行前完全退出 Dev。测试通过应用 IPC 验证下载、普通退出、离线恢复和递增签名包的真实替换与重启。结束后退出升级后的 Dev，运行 `node tests/e2e/restore-dev-after-update.mjs <evidence-directory>` 恢复原 Dev 外壳；该操作不回滚数据。`--application` 验收使用 Main 的完整安装协调，未指定该参数时仅探测原生更新器。

Dev 打包应用只有设置 `VIGILSHELL_DEV_UPDATE_FEED` 时才开放内部下载，且地址限于 HTTP loopback；正式构建忽略该变量，固定使用公开 Releases。原生热更新外壳不执行应用替换。Dev 和正式更新缓存使用独立名称，发行元数据不会决定本机数据或缓存目录。
