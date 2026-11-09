import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { describe, expect, it } from 'vitest';
import fixture from '../../tests/fixtures/local-execution-contract.json';
import type { ConnectionPolicy, ExecutionRequest, ExecutionState } from './index';
import { validateConnectionPolicy } from './generated/renderer-validation';
import { semanticRule } from './semantic-validation';
import { contractValidator } from './validation';

const validators = {
  ConnectionPolicy: contractValidator('ConnectionPolicy'),
  ExecutionRequest: contractValidator('ExecutionRequest'),
  ScriptReviewBinding: contractValidator('ScriptReviewBinding'),
  ExecutionState: contractValidator('ExecutionState'),
  ExecutionOutcome: contractValidator('ExecutionOutcome'),
};
const policy = fixture.schemaCases.find(
  (entry) => entry.type === 'ConnectionPolicy' && entry.valid,
)!.value as ConnectionPolicy;
const request = fixture.schemaCases.find(
  (entry) => entry.type === 'ExecutionRequest' && entry.valid,
)!.value as ExecutionRequest;
const binding = fixture.schemaCases.find(
  (entry) => entry.type === 'ScriptReviewBinding' && entry.valid,
)!.value;

describe('shared Rust and TypeScript execution contract cases', () => {
  for (const entry of fixture.schemaCases) {
    it(entry.name, () => {
      const before = structuredClone(entry.value);
      const validate = validators[entry.type as keyof typeof validators];
      expect(validate(entry.value), JSON.stringify(validate.errors)).toBe(entry.valid);
      expect(entry.value).toEqual(before);
      if (entry.type === 'ConnectionPolicy')
        expect(validateConnectionPolicy(entry.value)).toBe(entry.valid);
    });
  }
  for (const entry of fixture.authorizationCases) {
    it(`preserves Rust authorization case inputs: ${entry.name}`, () => {
      expect(validators.ExecutionRequest(entry.request)).toBe(true);
      expect(validators.ConnectionPolicy(entry.policy)).toBe(true);
      if ('expected' in entry) {
        expect(
          contractValidator('ExecutionAuthorization')({ ...entry.expected, targets: [] }),
        ).toBe(true);
      }
    });
  }
});

it('loads the new Renderer policy validator while string code generation is prohibited', () => {
  const source = readFileSync(
    new URL('./generated/renderer-validation.js', import.meta.url),
    'utf8',
  );
  const code = source
    .replace(/^import .*;\n/m, '')
    .replace('export const validateConnectionPolicy', 'const validateConnectionPolicy');
  const validate = runInNewContext(
    `${code}\nvalidateConnectionPolicy`,
    { semanticRule },
    { contextCodeGeneration: { strings: false, wasm: false } },
  );
  expect(validate(policy)).toBe(true);
  expect(validate({ ...policy, execution: { ...policy.execution, commandNetwork: false } })).toBe(
    false,
  );
  expect(
    validate({
      ...policy,
      execution: {
        ...policy.execution,
        limits: { ...policy.execution.limits, concurrentTasks: 0 },
      },
    }),
  ).toBe(false);
});

it('does not coerce types, default omitted scriptReview, or remove unknown fields', () => {
  for (const value of [null, {}, false, [], 'policy', { ...policy, revision: '3' }]) {
    const before = structuredClone(value);
    expect(validateConnectionPolicy(value)).toBe(false);
    expect(value).toEqual(before);
  }
  const omitted = structuredClone(policy);
  delete omitted.execution.scriptReview;
  expect(validateConnectionPolicy(omitted)).toBe(true);
  expect(Object.hasOwn(omitted.execution, 'scriptReview')).toBe(false);
});

it('enforces UTF-8 budgets rather than UTF-16 character counts', () => {
  const validateRequest = validators.ExecutionRequest;
  const directory = { scopeId: '__account__', path: 'srv' };
  for (const [command, valid] of [
    ['字'.repeat(21_845), true],
    ['字'.repeat(21_846), false],
  ] as const)
    expect(
      validateRequest({ ...request, operation: { kind: 'command', command, directory } }),
    ).toBe(valid);
  expect(
    validateRequest({
      ...request,
      operation: {
        kind: 'script',
        source: { kind: 'inline', text: 'echo ok' },
        interpreter: 'sh',
        directory,
        arguments: ['字'.repeat(21_846)],
      },
    }),
  ).toBe(false);
  expect(
    validateRequest({
      ...request,
      operation: {
        kind: 'files_write',
        target: directory,
        data: '字'.repeat(43_691),
        mode: 'create',
      },
    }),
  ).toBe(false);
  expect(
    validateRequest({
      ...request,
      operation: {
        kind: 'files_edit',
        target: directory,
        expected_hash: 'a'.repeat(64),
        edits: [{ before: '字'.repeat(349_526), after: '' }],
      },
    }),
  ).toBe(false);
});

it('rejects malformed semantic keyword inputs without throwing across the protocol boundary', () => {
  for (const rule of [
    'executionPolicy',
    'executionRequest',
    'scriptReviewBinding',
    'executionState',
  ]) {
    for (const value of [null, false, 1, '', [], {}]) expect(semanticRule(rule, value)).toBe(false);
  }
});

it('checks every review, dispatch, phase and outcome combination against the Domain truth table', () => {
  const phases = ['waitingReview', 'queued', 'dispatching', 'running', 'stopping', 'settled'];
  const reviews = [
    { kind: 'notRequired' },
    ...['waitingReview', 'approved', 'rejected'].map((kind) => ({ kind, binding })),
  ];
  const dispatches = [
    { kind: 'notDispatched' },
    { kind: 'attempted', attemptId: 'attempt', attemptedAt: 1 },
  ];
  const outcomes = [
    null,
    { kind: 'notExecuted', reason: 'denied' },
    { kind: 'notExecuted', reason: 'restarted' },
    { kind: 'completed', exitCode: 0 },
    { kind: 'requestAccepted' },
    { kind: 'partial', reason: 'cancelled', terminationConfirmed: false },
    { kind: 'unknown', reason: 'disconnected' },
  ];
  for (const phase of phases)
    for (const review of reviews)
      for (const dispatch of dispatches)
        for (const outcome of outcomes)
          for (const terminal of [false, true]) {
            const permitted = review.kind === 'notRequired' || review.kind === 'approved';
            const attempted = dispatch.kind === 'attempted';
            let valid: boolean;
            if (phase === 'waitingReview')
              valid = review.kind === 'waitingReview' && !attempted && outcome === null;
            else if (phase === 'queued') valid = permitted && !attempted && outcome === null;
            else if (phase === 'settled')
              valid =
                outcome !== null &&
                (outcome.kind === 'notExecuted'
                  ? !attempted && (review.kind !== 'rejected' || outcome.reason === 'denied')
                  : attempted && permitted);
            else valid = permitted && attempted && outcome === null;
            valid &&= !terminal || review.kind === 'notRequired';
            const value = { phase, review, dispatch, outcome, terminal };
            expect(validators.ExecutionState(value), JSON.stringify(value)).toBe(valid);
          }
});

it('keeps outcomes and historical records separate from new execution requests', () => {
  expect(validators.ExecutionRequest({ kind: 'requestAccepted' })).toBe(false);
  expect(validators.ExecutionOutcome({ kind: 'requestAccepted', terminationConfirmed: true })).toBe(
    false,
  );
  const historical = {
    id: 'old',
    entryId: 'entry',
    connectionId: 'connection',
    createdAt: 1,
    active: false,
    rawBody: JSON.stringify({ protocol: 8, executorPid: 123, approved: true }),
    display: {},
  };
  expect(contractValidator('HistoricalTask')(historical)).toBe(true);
  expect(validators.ExecutionRequest(historical)).toBe(false);
  const omittedOutcome = {
    phase: 'queued',
    review: { kind: 'notRequired' },
    dispatch: { kind: 'notDispatched' },
    terminal: false,
  } satisfies ExecutionState;
  expect(validators.ExecutionState(omittedOutcome)).toBe(true);
});
