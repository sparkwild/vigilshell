import { isWorkspaceReply } from '../../tests/helpers/protocol-validation';
import { expect, it } from 'vitest';
import { validWorkspaceCommand, isWorkspaceNotification } from './workspace-validation';
const id = '11111111-1111-4111-8111-111111111111';
it('accepts opaque draft handles and rejects secrets and owner injection', () => {
  const valid = {
    method: 'credentialDrafts.configure',
    params: { draftId: id, auth: 'password' },
  };
  expect(validWorkspaceCommand(valid)).toBe(true);
  for (const extra of [
    { password: 'synthetic' },
    { privateKey: 'synthetic' },
    { owner: id },
    { passphrase: 'synthetic' },
  ])
    expect(validWorkspaceCommand({ ...valid, params: { ...valid.params, ...extra } })).toBe(false);
});
it('validates workspace negotiation, control ranges and runtime envelopes', () => {
  expect(
    validWorkspaceCommand({
      method: 'workspace.negotiate',
      params: { version: 16 },
    }),
  ).toBe(true);
  expect(
    validWorkspaceCommand({
      method: 'workspace.negotiate',
      params: { version: 7 },
    }),
  ).toBe(false);
  expect(
    validWorkspaceCommand({
      method: 'terminals.resize',
      params: { sessionId: id, columns: 0, rows: 24 },
    }),
  ).toBe(false);
  expect(
    isWorkspaceReply({
      kind: 'ready',
      version: 16,
      connectionAccess: { phase: 'locked', revision: 0, code: null },
    }),
  ).toBe(true);
  expect(isWorkspaceReply({ kind: 'ready', version: 7 })).toBe(false);
  expect(isWorkspaceReply({ kind: 'ready', version: 16, secret: 'synthetic' })).toBe(false);
  expect(
    isWorkspaceNotification({
      jsonrpc: '2.0',
      method: 'workspace.changed',
      params: { runtimeId: 'runtime', owner: id, event: { kind: 'catalogue' } },
    }),
  ).toBe(true);
});

import examples from '../../tests/fixtures/workspace-protocol-v16.json';
it('shares structural workspace conformance cases with Rust', () => {
  for (const value of examples.valid) expect(validWorkspaceCommand(value)).toBe(true);
  for (const value of examples.invalid) expect(validWorkspaceCommand(value)).toBe(false);
});

it('accepts draft-only clear and rejects the retired immediate-clear command', () => {
  expect(
    validWorkspaceCommand({
      method: 'credentialDrafts.clear',
      params: { draftId: id },
    }),
  ).toBe(true);
  expect(
    validWorkspaceCommand({
      method: 'credentialDrafts.clear',
      params: { draftId: id, owner: id },
    }),
  ).toBe(false);
  expect(
    validWorkspaceCommand({
      method: 'credentials.clear',
      params: { id, version: 1 },
    }),
  ).toBe(false);
  for (const intent of ['keep', 'replace', 'remove']) {
    expect(
      isWorkspaceReply({
        kind: 'draft',
        draft: {
          id,
          connectionId: id,
          proxyId: null,
          intent,
          prepared: intent === 'replace',
          saved: true,
          auth: 'password',
          label: null,
        },
      }),
    ).toBe(true);
  }
});

it('restricts local file grants to the desktop main process', () => {
  expect(
    validWorkspaceCommand({
      method: 'files.pick',
      params: { direction: 'upload' },
    }),
  ).toBe(true);
  expect(
    validWorkspaceCommand({
      method: 'files.pick',
      params: { direction: 'upload', paths: ['/private/file'] },
    }),
  ).toBe(false);
  expect(
    validWorkspaceCommand({
      method: 'files.grant',
      params: { direction: 'upload', paths: ['/private/file'] },
    }),
  ).toBe(false);
  expect(
    validWorkspaceCommand({
      method: 'files.transfer',
      params: {
        sessionId: id,
        grantId: id,
        paths: ['/home/目录'],
        overwrite: false,
      },
    }),
  ).toBe(true);
  expect(
    validWorkspaceCommand({
      method: 'files.remove',
      params: {
        sessionId: id,
        path: '/folder',
        directory: true,
        recursive: true,
      },
    }),
  ).toBe(false);
});
it('validates file lifecycle and conflict decisions without exposing local paths', () => {
  expect(
    validWorkspaceCommand({
      method: 'files.releaseList',
      params: { requestId: id },
    }),
  ).toBe(true);
  expect(
    validWorkspaceCommand({
      method: 'files.identities',
      params: { sessionId: id, uids: [0, 1000], gids: [0] },
    }),
  ).toBe(true);
  expect(
    validWorkspaceCommand({
      method: 'files.identities',
      params: { sessionId: id, uids: ['0;whoami'], gids: [] },
    }),
  ).toBe(false);
  expect(
    validWorkspaceCommand({
      method: 'transfers.resolveConflict',
      params: { id, version: 1, decision: 'overwrite', batch: true },
    }),
  ).toBe(true);
  expect(
    validWorkspaceCommand({
      method: 'transfers.resolveConflict',
      params: {
        id,
        version: 1,
        decision: 'overwrite',
        batch: true,
        localPath: '/tmp/private',
      },
    }),
  ).toBe(false);
  expect(
    validWorkspaceCommand({
      method: 'workspace.negotiate',
      params: { version: 4 },
    }),
  ).toBe(false);
});
