## Features

- **In-app macOS updates** — Check for stable releases under Settings → About, download an update, and choose Restart and install. Downloaded updates remain available after restarting VigilShell.
- **Update progress and safeguards** — View download progress or cancel a download. VigilShell verifies update metadata and downloaded files, checks installation space, and saves pending application state before restarting to install.

## Bug Fixes

- **Dependency maintenance** — Update the SSH and filesystem dependencies to include upstream security fixes.

- **Connection test feedback** — Connection tests now show their results reliably. Success messages apply to the tested configuration, and running another test can show its result again after an earlier message was dismissed.
- **Connection notices** — Keep the close button aligned with the notice header, including notices with additional actions or details.

## Upgrade Notes

- This version was not published. Its source tag is retained; the planned features and fixes are included in 0.1.3.

- Upgrade from 0.1.1 or earlier by downloading the DMG and replacing the application manually. The first published version with the in-app update engine is 0.1.3.
- Update checks and downloads are initiated manually. An ordinary quit does not install a downloaded update; use Restart and install when you are ready.
