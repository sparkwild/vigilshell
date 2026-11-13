# Changelog

See [GitHub Releases](https://github.com/sparkwild/vigilshell/releases) for published installers.

## v0.1.3

### Features

- **In-app macOS updates** — Check for stable releases under Settings → About, download an update, and choose Restart and install. Downloaded updates remain available after restarting VigilShell.
- **Update progress and safeguards** — View download progress or cancel a download. VigilShell verifies update metadata and downloaded files, checks installation space, and saves pending application state before restarting to install.

### Bug Fixes

- **Dependency maintenance** — Update the SSH and filesystem dependencies to include upstream security fixes.

- **Connection test feedback** — Connection tests now show their results reliably. Success messages apply to the tested configuration, and running another test can show its result again after an earlier message was dismissed.
- **Connection notices** — Keep the close button aligned with the notice header, including notices with additional actions or details.

### Upgrade Notes

- This release includes the features and fixes prepared for 0.1.2. The 0.1.2 tag is retained as a source baseline, but no 0.1.2 installer was published.

- Upgrade from 0.1.1 or earlier by downloading the DMG and replacing the application manually. The in-app update engine is available starting with 0.1.3 for subsequent releases.
- Update checks and downloads are initiated manually. An ordinary quit does not install a downloaded update; use Restart and install when you are ready.

## v0.1.2

### Features

- **In-app macOS updates** — Check for stable releases under Settings → About, download an update, and choose Restart and install. Downloaded updates remain available after restarting VigilShell.
- **Update progress and safeguards** — View download progress or cancel a download. VigilShell verifies update metadata and downloaded files, checks installation space, and saves pending application state before restarting to install.

### Bug Fixes

- **Dependency maintenance** — Update the SSH and filesystem dependencies to include upstream security fixes.

- **Connection test feedback** — Connection tests now show their results reliably. Success messages apply to the tested configuration, and running another test can show its result again after an earlier message was dismissed.
- **Connection notices** — Keep the close button aligned with the notice header, including notices with additional actions or details.

### Upgrade Notes

- This version was not published. Its source tag is retained; the planned features and fixes are included in 0.1.3.

- Upgrade from 0.1.1 or earlier by downloading the DMG and replacing the application manually. The first published version with the in-app update engine is 0.1.3.
- Update checks and downloads are initiated manually. An ordinary quit does not install a downloaded update; use Restart and install when you are ready.

## v0.1.1

### Features

- **SSH and terminal** — Organize SSH connections, verify host fingerprints, and work across multiple terminal tabs.
- **Remote files** — Browse and transfer files over SFTP, or edit remote text files in a local editor with conflict detection.
- **Server monitoring** — View CPU, memory, disk, and network activity on connected Linux hosts.
- **MCP and agent activity** — Connect an MCP client, grant access per SSH connection, and review agent tasks and output in VigilShell.

### Bug Fixes

- **Smaller macOS installer** — The Apple Silicon download removes duplicate frontend dependencies and unused development files while retaining the app's fonts and features.
- **Chromium notices** — Find the full Chromium license notices beside the app in the disk image.
- **Startup screen** — The loading indicator and its label stay aligned.

## v0.1.0

### Features

- **SSH and terminal** — Organize SSH connections, verify host fingerprints, and work across multiple terminal tabs.
- **Remote files** — Browse and transfer files over SFTP, or edit remote text files in a local editor with conflict detection.
- **Server monitoring** — View CPU, memory, disk, and network activity on connected Linux hosts.
- **MCP and agent activity** — Connect an MCP client, grant access per SSH connection, and review agent tasks and output in VigilShell.
