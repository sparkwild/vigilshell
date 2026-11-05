import { expect, it } from 'vitest';
import { contractValidator } from './validation';

it('accepts DSH in discovery and access setup while rejecting unknown client identities', () => {
  const detection = contractValidator('AgentClientDetection');
  const request = contractValidator('AgentUiRequest');
  const result = { client: 'dsh', installed: true, configurationFound: false, unavailable: false };
  const setup = {
    action: 'prepareAccess',
    client: 'dsh',
    name: 'DSH',
    id: null,
    delivery: 'prompt',
    requestKey: 'discovery-test',
  };
  expect(detection(result)).toBe(true);
  expect(request(setup)).toBe(true);
  expect(detection({ ...result, client: 'unknown' })).toBe(false);
  expect(request({ ...setup, client: 'unknown' })).toBe(false);
});
