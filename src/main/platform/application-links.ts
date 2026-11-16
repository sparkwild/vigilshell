import { shell } from 'electron';

export function openApplicationLink(page: 'project' | 'issues') {
  const root = 'https://github.com/sparkwild/vigilshell';
  return shell.openExternal(page === 'issues' ? `${root}/issues` : root);
}
