import type {
  WorkspaceCommand,
  WorkspaceReply,
  WorkspaceNotification,
  TerminalSessionView,
} from '@contracts';
export const workspaceMethods = {
  statFile: 'files.stat',
  createFile: 'files.create',
  startFileOperation: 'files.startOperation',
  getFileOperation: 'files.operation',
  retryFileOperation: 'files.retryOperation',
  subscribeTools: 'tools.subscribe',
  refreshTools: 'tools.refresh',
  listFiles: 'files.list',
  releaseFileList: 'files.releaseList',
  resolveFileIdentities: 'files.identities',
  resolveTransferConflict: 'transfers.resolveConflict',
  cancelFileList: 'files.cancelList',
  createDirectory: 'files.mkdir',
  renameFile: 'files.rename',
  removeFile: 'files.remove',
  pickFiles: 'files.pick',
  startTransfer: 'files.transfer',
  archiveFiles: 'files.archive',
  listTransferItems: 'transfers.items',
  cancelTransfer: 'transfers.cancel',
  retryTransfer: 'transfers.retry',
  listTransfers: 'transfers.list',
  listProxies: 'proxies.list',
  proxyReferences: 'proxies.references',
  saveProxy: 'proxies.save',
  deleteProxy: 'proxies.delete',
  beginProxyDraft: 'proxyCredentialDrafts.begin',
  listTunnels: 'tunnels.list',
  retryTunnel: 'tunnels.retry',
  validateTerminalInput: 'terminals.validateInput',
  createGroup: 'groups.create',
  updateGroup: 'groups.update',
  deleteGroup: 'groups.delete',
  updateLayout: 'groups.updateLayout',
  listCatalogue: 'catalogue.list',
  unlockConnections: 'connections.unlock',
  createConnection: 'connections.create',
  updateConnection: 'connections.update',
  getConnection: 'connections.get',
  deleteConnection: 'connections.delete',
  reconcileCredentials: 'credentials.reconcile',
  beginDraft: 'credentialDrafts.begin',
  configureDraft: 'credentialDrafts.configure',
  clearDraft: 'credentialDrafts.clear',
  discardDraft: 'credentialDrafts.discard',
  testConnection: 'connections.test',
  confirmTrust: 'hostTrust.confirm',
  rejectTrust: 'hostTrust.reject',
  forgetTrust: 'hostTrust.forget',
  openTerminal: 'terminals.open',
  listTerminals: 'terminals.list',
  resizeTerminal: 'terminals.resize',
  closeTerminal: 'terminals.close',
  getOperation: 'operations.get',
  cancelOperation: 'operations.cancel',
  reconcile: 'connections.reconcile',
} as const satisfies Record<string, WorkspaceCommand['method']>;
export type WorkspaceApiName = keyof typeof workspaceMethods;
export type WorkspaceCallResult =
  | { ok: true; value: WorkspaceReply }
  | { ok: false; code: string; requestId?: string; runtimeId?: string };
export type WorkspaceApi = {
  [K in WorkspaceApiName]: (
    params: Extract<WorkspaceCommand, { method: (typeof workspaceMethods)[K] }>['params'],
  ) => Promise<WorkspaceCallResult>;
};
export interface WorkspaceUiState {
  dirty: boolean;
  modalOpen: boolean;
  activeSessionId: string | null;
  active: boolean;
}
export interface WorkspaceDesktop {
  getConnectionAccess(): Promise<import('@contracts').ConnectionAccessSnapshot>;
  onConnectionAccessChanged(
    listener: (value: import('@contracts').ConnectionAccessSnapshot) => void,
  ): () => void;
  reportTransferFeedback(report: import('@contracts').TransferFeedbackReport): void;
  onTransferNavigation(
    listener: (target: import('@contracts').TransferNotificationTarget) => void,
  ): () => void;
  workspace: WorkspaceApi;
  onWorkspaceEvent(listener: (event: WorkspaceNotification) => void): () => void;
  onWorkspaceCommand(listener: (command: 'new' | 'close' | 'find') => void): () => void;
  attachTerminalPort(sessionId: string): void;
  setWorkspaceUi(state: WorkspaceUiState): void;
  copyText(text: string): Promise<void>;
  readClipboard(): Promise<string>;
  hideWindow(): void;
}
export const terminalIsActive = (s: TerminalSessionView) =>
  !['closed', 'failed', 'cancelled'].includes(s.phase);
