# Changelog

See [GitHub Releases](https://github.com/sparkwild/vigilshell/releases) for published installers.

## v0.1.4

### Features

- **Background update checks** — Check for new versions automatically by default and open update details from the workspace or About.
- **Localized release notes** — Read release notes in the app's current language, falling back to the default language when the notes are missing or incomplete.

### Bug Fixes

- **Update dialog** — Reduce excess spacing and scroll long release notes independently while keeping version information, download progress, and actions visible.
- **Close buttons** — Restore compact close buttons in connection notices, search bars, and transfer popovers.
- **macOS installer** — Improve the drag-to-install window layout.

### Upgrade Notes

- Users on 0.1.3 can use in-app updates; users on earlier published versions must download the DMG and replace the app manually.

## v0.1.3

### Features

- **In-app macOS updates** — Download stable updates in VigilShell and restart to install when ready.
- **Download progress** — Track or cancel downloads and keep completed downloads available after restarting the app.

### Bug Fixes

- **Security updates** — Include upstream security fixes for SSH connections and local file operations.
- **Connection tests** — Show results for the tested configuration reliably, including after an earlier result was dismissed.
- **Connection notices** — Keep close buttons aligned when notices include actions or details.

### Upgrade Notes

- Users on 0.1.1 or earlier must install 0.1.3 manually; subsequent releases can be installed through in-app updates.

## v0.1.2

### Features

- **In-app macOS updates** — Check for stable releases under Settings → About, download an update, and choose Restart and install. Downloaded updates remain available after restarting VigilShell.
- **Update progress and safeguards** — Track or cancel downloads and get a warning if there is not enough disk space to install.

### Bug Fixes

- **Security updates** — Fix security issues affecting SSH connections and local file operations.

- **Connection test feedback** — Connection tests now show their results reliably. Success messages apply to the tested configuration, and running another test can show its result again after an earlier message was dismissed.
- **Connection notices** — Keep the close button aligned with the notice header, including notices with additional actions or details.

### Upgrade Notes

- This version was not published; its features and fixes are included in 0.1.3.

## v0.1.1

### Features

- **SSH and terminal** — Organize SSH connections, verify host fingerprints, and work across multiple terminal tabs.
- **Remote files** — Browse and transfer files over SFTP, or edit remote text files in a local editor with conflict detection.
- **Server monitoring** — View CPU, memory, disk, and network activity on connected Linux hosts.
- **MCP and agent activity** — Connect an MCP client, grant access per SSH connection, and review agent tasks and output in VigilShell.

### Bug Fixes

- **Smaller macOS installer** — Reduce the Apple Silicon installer download size.
- **Chromium notices** — Find the full Chromium license notices beside the app in the disk image.
- **Startup screen** — The loading indicator and its label stay aligned.

## v0.1.0

### Features

- **SSH and terminal** — Organize SSH connections, verify host fingerprints, and work across multiple terminal tabs.
- **Remote files** — Browse and transfer files over SFTP, or edit remote text files in a local editor with conflict detection.
- **Server monitoring** — View CPU, memory, disk, and network activity on connected Linux hosts.
- **MCP and agent activity** — Connect an MCP client, grant access per SSH connection, and review agent tasks and output in VigilShell.
