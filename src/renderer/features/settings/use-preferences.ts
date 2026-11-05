import { useState } from 'react';
import type { PreferencesPatch } from '@contracts';
import { useDesktop } from '../../app/DesktopProvider';
import { defaultPreferences } from '../../../shared/preferences';
export function usePreferences() {
  const { store, snapshot } = useDesktop();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [failedPatch, setFailedPatch] = useState<Omit<
    PreferencesPatch,
    'downloadDirectory'
  > | null>(null);
  const disabled =
    saving ||
    snapshot.runtime.status !== 'ready' ||
    !snapshot.preferences ||
    snapshot.save.phase !== 'idle';
  async function save(patch: Omit<PreferencesPatch, 'downloadDirectory'>) {
    setError(null);
    setFailedPatch(null);
    setSaving(true);
    try {
      const result = await store.updateFields(patch);
      setError(result.error);
      if (result.error === 'storage_unavailable' && result.save.phase === 'idle')
        setFailedPatch(patch);
      return !result.error;
    } catch {
      setError('host_unavailable');
      return false;
    } finally {
      setSaving(false);
    }
  }
  return {
    preferences: snapshot.preferences ?? defaultPreferences,
    disabled,
    save,
    error,
    setError,
    retry: failedPatch && !disabled ? () => save(failedPatch) : null,
  };
}
