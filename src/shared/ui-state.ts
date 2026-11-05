import type { UiStateData } from '@contracts';
import { contractValidator } from '../contracts/validation';
import { validUiScope } from '../contracts/semantic-validation';
const data = contractValidator<UiStateData>('UiStateData');
export const uiScopeSchema = {
  parse(value: unknown): string {
    if (!validUiScope(value)) throw Error('invalid_scope');
    return value;
  },
};
export const uiStateDataSchema = {
  parse(value: unknown): UiStateData {
    if (!data(value)) throw Error('invalid_ui_state');
    return value;
  },
};
export type UiStateNotice = {
  scope: string;
  data: UiStateData | null;
  error: string | null;
};
export type UiStatePatch = {
  [K in UiStateData['kind']]: { kind: K } & Partial<
    Omit<Extract<UiStateData, { kind: K }>, 'kind'>
  >;
}[UiStateData['kind']];
