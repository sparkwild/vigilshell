import type { AuthenticationMethod, CredentialDraftView } from '@contracts';

export function credentialEditorState(
  auth: AuthenticationMethod,
  saved: {
    credentialSaved: boolean;
    config: { auth: AuthenticationMethod };
  } | null,
  draft: CredentialDraftView | null,
) {
  const removing = draft?.intent === 'remove';
  return {
    ready:
      !removing &&
      (draft?.prepared
        ? draft.auth === auth
        : !!saved?.credentialSaved && saved.config.auth === auth),
    dirty: !!draft?.prepared || removing,
    removalPending: removing && draft.saved,
  };
}
