import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { expect, it } from 'vitest';
import { validateConnectionPolicy } from './generated/renderer-validation';
import { semanticRule } from './semantic-validation';
import { contractValidator } from './validation';
import { defaultPolicy } from '../renderer/features/mcp/model';

it('loads and validates with string code generation prohibited', () => {
  const source = readFileSync(
    new URL('./generated/renderer-validation.js', import.meta.url),
    'utf8',
  );
  const data = defaultPolicy('connection');
  const result = runInNewContext(
    `${source.replace("import { semanticRule } from '../semantic-validation';", '').replace('export const validateConnectionPolicy', 'const validateConnectionPolicy')}\nvalidateConnectionPolicy(data)`,
    { data, semanticRule },
    { contextCodeGeneration: { strings: false, wasm: false } },
  );
  expect(result).toBe(true);
});

it('matches the authoritative runtime schema on policy structure and never coerces input', () => {
  const dynamic = contractValidator('ConnectionPolicy');
  const base = defaultPolicy('connection');
  const cases: unknown[] = [base, null, {}, { ...base, extra: true }, { ...base, enabled: 'true' }];
  for (const field of Object.keys(base)) {
    const value = { ...base };
    delete value[field as keyof typeof value];
    cases.push(value);
  }
  for (const value of [0, -1, 4, 1.5, '4', null, Number.MAX_SAFE_INTEGER]) {
    const policy = structuredClone(base);
    Object.assign(policy.execution.limits, { concurrentTasks: value });
    cases.push(policy);
  }
  for (const value of cases) {
    const before = structuredClone(value);
    expect(validateConnectionPolicy(value)).toBe(dynamic(value));
    expect(value).toEqual(before);
  }
});
