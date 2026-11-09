# VigilShell

**English** | [简体中文](docs/README.zh-CN.md)

VigilShell is an SSH and server management desktop application for macOS. It brings terminals, remote files, server monitoring, and agent operations into one workspace.

Connect to servers yourself, or use the built-in MCP service to let an agent work with your connections and run tasks under configured permissions. You can inspect its activity and results in the application.

## Features

| Area                | Available functionality                                                                                                                                                                                                         |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| SSH connections     | Password and private key authentication, connection testing, and host fingerprint verification; connection groups, environment labels, search, and sorting                                                                      |
| Terminal workspace  | Multiple session tabs, terminal search, character encoding settings, multiline paste confirmation, and manual reconnection                                                                                                      |
| Proxies and tunnels | SOCKS5 and HTTP CONNECT proxies; local, remote, and dynamic SOCKS5 port forwarding                                                                                                                                              |
| Remote files        | SFTP browsing, file and directory uploads and downloads, creation, renaming, deletion, and permission changes; transfer progress, name conflict handling, and failure details                                                   |
| External editors    | Open remote files in a local editor and sync saved changes back to the server; detect remote version conflicts and retain pending local changes                                                                                 |
| Server monitoring   | Host information, CPU, memory, disk usage, and network traffic; monitoring currently targets Linux servers                                                                                                                      |
| MCP and agents      | A local Streamable HTTP service, separate access tokens, and permissions configured per connection; commands, scripts, terminals, files, and service, package, and system maintenance where supported by the remote environment |
| Activity visibility | Agent session views, task status, output, and activity records; release terminal control or end an agent session                                                                                                                |
| Desktop experience  | English and Simplified Chinese, light/dark/system themes, a menu bar entry, and system notifications                                                                                                                            |

MCP operations are subject to connection authorization and remote account permissions. Available capabilities also depend on the server environment. VigilShell provides tools for external MCP clients; it does not include a language model.

## Installation and running

Public release installers are not available yet. The native development entry point and packaging scripts currently target macOS. You can run from source or build a DMG locally.

- **Run from source:** follow [Development](#development) to set up dependencies and signing, then start `VigilShell Dev`.
- **Install a local build:** after [building a package](#building-a-package), open the DMG in `release/` and drag `VigilShell.app` into Applications.
- **Architectures:** the packaging scripts support Apple Silicon (`arm64`) and Intel (`x64`). Each installer must be built with macOS, Node.js, and Rust running on the corresponding architecture.

Current builds use a fixed self-signed certificate and are not notarized by Apple. Launching a build on another Mac depends on that machine's certificate trust and security checks. A successful local build does not establish readiness for public distribution.

## Usage

### Connect over SSH

1. Start the application and complete any system authorization prompts to unlock connection settings.
2. Choose **New connection** in the sidebar. Enter the server address, port, and username, then select password or private key authentication.
3. Configure terminal encoding, a proxy, or tunnels in the connection editor if needed.
4. Use **Test connection** to check authentication. Verify the server fingerprint before trusting a new host or accepting a changed host key.
5. Choose **Save and connect** to open a terminal. Existing connections can open additional session tabs and can be organized using groups and environment labels.

**Test connection** checks authentication and disconnects afterward; it does not open a terminal.

### Files and monitoring

Once connected, use the file pane below the terminal to browse remote directories, upload and download files, or access file operations from the context menu. Transfer lists show progress, conflicts, and failed items. When closing a terminal, you can choose to keep unfinished transfers running in the background.

To edit remote text files, select a local editor under **Settings → Files & transfers**, then open a file from its context menu. VigilShell attempts to sync changes after the editor saves them. If the remote content has changed, resolve the conflict or save a local copy from **Editing files**.

The **Overview** tab in the tools panel on the right shows server information and monitoring data. Sampling follows the connection and panel visibility state. Adjust sampling intervals under **Settings → Monitoring**. Metrics unsupported by the remote environment may be unavailable.

### Connect an agent

1. Open **Settings → MCP Service**, enable the local service, and add a connection. Choose a detected client or **Generic Agent**.
2. Select **Let Agent configure** to copy setup instructions, or copy the configuration and adapt it to the client's MCP settings. The client must support Streamable HTTP and the `Authorization` header.
3. Right-click an SSH connection in the sidebar and choose **Set Agent permissions**. In **Agent permission management**, select **Read only** or **Operate**, configure any path protections, transfer directories, and task limits, then enable access.
4. Start a task from the client and complete session identity verification when required by the configuration. View agent sessions in the workspace and tasks and output under **Settings → Agent activity**.

The service listens only on the local loopback interface, using port `51602` by default. Use the actual endpoint shown in settings and included in the copied configuration. That configuration contains an access token; keep it in your client's configuration and out of source control.

Closing the window hides the application, so the MCP service can continue running. Choose **Quit VigilShell** from the application menu to exit completely.

## Development

### Requirements

| Tool               | Requirement                                                                                    |
| ------------------ | ---------------------------------------------------------------------------------------------- |
| Operating system   | macOS; native development and packaging depend on system signing, Keychain, and native bridges |
| Node.js            | `24.21.0`; see [.nvmrc](.nvmrc)                                                                |
| npm                | `11.19.1`; see [package.json](package.json)                                                    |
| Rust               | `1.96.0` with `rustfmt` and `clippy`; see [rust-toolchain.toml](rust-toolchain.toml)           |
| Native build tools | Xcode Command Line Tools                                                                       |
| Signing identity   | A usable code-signing certificate and its private key in a keychain                            |

The commands below assume Git, nvm, rustup, and Xcode Command Line Tools are installed and that you have access to the repository.

### Get the source and dependencies

```sh
git clone https://github.com/sparkwild/vigilshell.git
cd vigilshell

nvm install
nvm use
npm install --global npm@11.19.1
npm ci

rustup toolchain install 1.96.0 --profile minimal --component rustfmt --component clippy
```

Use the repository's `package-lock.json` and `Cargo.lock` when installing dependencies. See the [technology stack](docs/TECH_STACK.md) for the full dependency overview (in Chinese).

### Configure development signing

List the code-signing identities available on your machine:

```sh
security find-identity -v -p codesigning
```

If no usable identity is available, first create or import a code-signing certificate and private key in Keychain. The configuration command does not generate certificates, and ad hoc signing cannot replace the fixed identity.

Replace the placeholders below with the certificate's **40-character fingerprint** and the **absolute path** to its keychain:

```sh
npm run signing:configure -- --profile dev \
  --identity "<40-character-certificate-fingerprint>" \
  --keychain "/absolute/path/login.keychain-db"
npm run signing:check -- --profile dev
```

Signing configuration is stored in `~/Library/Application Support/VigilShell Signing/`. Keep using the existing identity once configured. Certificates and private keys stay outside the source repository. See [signing preparation](docs/TECH_STACK.md#签名准备) for details (in Chinese).

### Start the development application

```sh
npm run dev:app
```

This prepares the Rust backend, communication contracts, localization code, and native bridges, then starts `VigilShell Dev` with hot reload. Reuse the same signed development application for day-to-day work. Fully quit the running `VigilShell Dev` before rerunning the startup command or performing a full build.

### Common commands

| Command                      | Purpose                                                                                                                 |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `npm run dev:app`            | Start the signed native development application with hot reload                                                         |
| `npm run check`              | Check dependencies, architecture boundaries, lint, formatting, types, contracts, localization, and Rust static analysis |
| `npm test`                   | Build the backend and run signing tests, Vitest, and Rust workspace tests                                               |
| `npm run backend:check`      | Run Rust formatting checks, Clippy, and tests with locked dependencies                                                  |
| `npm run contracts:generate` | Generate JSON Schema, TypeScript types, and runtime validators from Rust DTOs                                           |
| `npm run contracts:check`    | Check that generated communication contracts match their source                                                         |
| `npm run i18n:generate`      | Generate localization code from `messages/`                                                                             |
| `npm run build`              | Compile backend, native bridge, and frontend resources for a production build                                           |
| `npm run package:dev`        | Build a Dev application directory under `release/dev/`                                                                  |
| `npm run package:release`    | Build and verify a DMG using the release profile under `release/`                                                       |

Communication contracts are generated from Rust DTOs; localization code is generated from message sources. Edit the source and run the corresponding generator instead of editing generated files. Use Dev data for testing and debugging, preserving the production application's data and signing identity.

### Building a package

Prepare a separate `release` configuration using the same process as development signing. After selecting the appropriate signing identity, run:

```sh
npm run signing:configure -- --profile release \
  --identity "<40-character-release-certificate-fingerprint>" \
  --keychain "/absolute/path/login.keychain-db"
npm run signing:check -- --profile release
npm run package:release
```

Packaging defaults to the current Node.js process architecture. You can also specify `--arch arm64` or `--arch x64`. For example, in a native Apple Silicon environment:

```sh
npm run package:release -- --arch arm64
```

The target architecture must match the macOS host process and Rust toolchain. The script builds, signs, and verifies the DMG before placing artifacts in `release/`. It only creates local files; it does not upload installers or create a GitHub Release.

`npm run build` defaults to the release signing profile. Packaged builds keep their frontend, native bridges, backend resources, and Cargo output under `.cache/build/<dev|release>/<architecture>/`, separate from the live Dev application. Release builds require only the release signing configuration; they do not fall back to Dev credentials.

### ARM64 distribution and previews

The initial packaging target is macOS on Apple Silicon. Public version tags use `vX.Y.Z`. Actions builds temporary-signed previews and prepares Release drafts from verified stable tags. Maintainers publish drafts manually. Distributed packages use a fixed self-signed identity without Apple notarization. Update discovery opens the release page for manual installation.

In a separate test Mac/account with no existing VigilShell data, download the DMG and its SHA-256 file from the same release, verify with `shasum -a 256 -c <file>.dmg.sha256`, then drag the application into Applications. If macOS blocks an unverified developer, use the per-app **Privacy & Security → Open Anyway** flow after checking the download source; do not disable Gatekeeper globally.

For manual upgrades, fully quit VigilShell, replace the application with the next version, then verify saved settings and credential access. Initial download, first launch, credential prompts and upgrades between published versions require actual testing on an independent environment. Never use existing production data for testing.

## Data and credentials

Connection settings and credentials are encrypted and stored locally. Unlock keys are managed by macOS Keychain. The default data directories are separate for each application channel:

| Content                                         | Default location                                    |
| ----------------------------------------------- | --------------------------------------------------- |
| VigilShell data                                 | `~/Library/Application Support/VigilShell/`         |
| VigilShell Dev data                             | `~/Library/Application Support/VigilShell-dev/`     |
| Development and packaging signing configuration | `~/Library/Application Support/VigilShell Signing/` |

For development runs, `VIGILSHELL_DATA_DIR` can specify another absolute path. Source code and build caches are not a backup of user data. Restoring encrypted settings and credentials also requires the matching system key.

## Technology and architecture

The application separates Electron Main/Preload, a React Renderer, and a Rust Backend. The UI calls the backend through a controlled desktop API. Communication contracts are generated along the Rust DTO → JSON Schema → TypeScript/Ajv pipeline.

| Directory                       | Contents                                                               |
| ------------------------------- | ---------------------------------------------------------------------- |
| `src/renderer/`                 | React UI and workspace interactions                                    |
| `src/main/`, `src/preload/`     | Electron integration, IPC, MCP, and native bridges                     |
| `src/contracts/`, `src/shared/` | Desktop contracts and shared logic                                     |
| `crates/`                       | Rust domain logic, application services, protocols, and infrastructure |
| `scripts/`, `tests/`            | Development, build, validation, and test tools                         |
| `resources/`, `messages/`       | Application resources, third-party licenses, and localization messages |

The following technical documents are currently in Chinese:

- [Technology stack](docs/TECH_STACK.md): dependency versions, tooling, and signing preparation.
- [UI components and design](docs/UI_DESIGN.md): component libraries, style tokens, and interaction conventions.
- [System architecture](docs/SYSTEM_ARCHITECTURE.md): processes, module responsibilities, and state ownership.

## License status

VigilShell original code is licensed under [Apache-2.0](LICENSE). See [resources/licenses/](resources/licenses/) for third-party notices. The DMG includes the complete `LICENSES.chromium.html` from Electron alongside the app.

The source repository includes development skills with separate notices. [Third-party source notices](THIRD_PARTY_NOTICES.md)

[Contributing and preview builds](CONTRIBUTING.md)
