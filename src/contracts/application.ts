import type { ApplicationInfo, ApplicationRequest, UpdateSnapshot } from '@contracts';
/** Main resolves the named external destinations. */
export interface ApplicationApi {
  info(): Promise<ApplicationInfo>;
  state(): Promise<UpdateSnapshot>;
  check(): Promise<UpdateSnapshot>;
  open(action: 'openRelease' | 'project' | 'issues'): Promise<void>;
  onChanged(listener: (state: UpdateSnapshot) => void): () => void;
}
export type { ApplicationRequest };
