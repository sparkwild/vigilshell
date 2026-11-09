import type { WorkspaceCommand, WorkspaceNotification } from '@contracts';
import { contractValidator } from '../contracts/validation';
const command = contractValidator<WorkspaceCommand>('WorkspaceCommand');
const notification = contractValidator<WorkspaceNotification>('WorkspaceNotification');

export function validWorkspaceCommand(value: unknown): value is WorkspaceCommand {
  if (!command(value) || Object.keys(value).some((k) => !['method', 'params'].includes(k)))
    return false;
  // Local file paths are granted by Main after a native picker, never Renderer.
  if (value.method === 'files.grant') return false;
  if (value.method === 'groups.create' || value.method === 'groups.update') {
    const name = value.params.name.trim();
    return name.length > 0 && new TextEncoder().encode(name).length <= 128 && !/\p{Cc}/u.test(name);
  }
  return true;
}
export function isWorkspaceNotification(value: unknown): value is WorkspaceNotification {
  return notification(value) && value.jsonrpc === '2.0' && value.method === 'workspace.changed';
}
