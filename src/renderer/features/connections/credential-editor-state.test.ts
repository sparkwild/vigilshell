import { expect, it } from 'vitest';
import type { CredentialDraftView } from '@contracts';
import { credentialEditorState } from './credential-editor-state';
const saved = { credentialSaved: true, config: { auth: 'password' as const } };
const draft: CredentialDraftView = {
  id: 'draft',
  connectionId: 'connection',
  proxyId: null,
  intent: 'remove',
  prepared: false,
  saved: true,
  auth: 'password',
  label: null,
};
it('marks saved credential removal dirty and prevents fallback across auth switches', () => {
  for (const auth of ['password', 'privateKey'] as const)
    expect(credentialEditorState(auth, saved, draft)).toEqual({
      ready: false,
      dirty: true,
      removalPending: true,
    });
});
it('discard restores saved credentials, replacement becomes usable only for its auth type', () => {
  expect(credentialEditorState('password', saved, null)).toEqual({
    ready: true,
    dirty: false,
    removalPending: false,
  });
  const replacement: CredentialDraftView = {
    ...draft,
    intent: 'replace',
    prepared: true,
    auth: 'privateKey',
  };
  expect(credentialEditorState('privateKey', saved, replacement).ready).toBe(true);
  expect(credentialEditorState('password', saved, replacement).ready).toBe(false);
});
it('clearing a new credential remains unsaved but does not claim a saved secret will be removed', () => {
  expect(
    credentialEditorState('password', null, {
      ...draft,
      connectionId: null,
      saved: false,
    }),
  ).toEqual({ ready: false, dirty: true, removalPending: false });
});
