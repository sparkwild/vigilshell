import { it, expect } from 'vitest';
import examples from '../../../tests/fixtures/desktop-protocol-v12.json';
import { isRuntimeRequest } from '../../../tests/helpers/protocol-validation';
it.each(examples)('$name follows the shared request contract', ({ request, valid }) => {
  expect(isRuntimeRequest(request)).toBe(valid);
});
