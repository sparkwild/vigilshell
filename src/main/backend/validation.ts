import type { RuntimeResponse, RuntimeNotification, UpdatePreferencesRequest } from '@contracts';
import { contractValidator } from '../../contracts/validation';

const response = contractValidator<RuntimeResponse>('RuntimeResponse');
const notification = contractValidator<RuntimeNotification>('RuntimeNotification');
const update = contractValidator<UpdatePreferencesRequest>('UpdatePreferencesRequest');
export const isPreferencesUpdate = (value: unknown): value is UpdatePreferencesRequest =>
  update(value);
export function isRuntimeResponse(value: unknown): value is RuntimeResponse {
  return response(value) && value.jsonrpc === '2.0' && (value.id === null || value.id.length > 0);
}
export function isRuntimeNotification(value: unknown): value is RuntimeNotification {
  return (
    notification(value) &&
    value.jsonrpc === '2.0' &&
    value.method === 'preferences.changed' &&
    value.params.runtimeId.length > 0
  );
}
