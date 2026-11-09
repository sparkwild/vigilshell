import { expect, it } from 'vitest';
import type { ConnectionSummary, TerminalSessionView } from '@contracts';
import { visibleConnections, hasSearchOrFilters } from './navigation';
import { defaultNavigation } from './workspace-store';
const connection = (id: string, name: string, createdAt: string | null) =>
  ({
    id,
    name,
    createdAt,
    host: 'fixture',
    environment: 'none',
    auth: 'password',
    credentialSaved: true,
    credentialPending: false,
    deleting: false,
    groupId: null,
    groupSortOrder: 0,
    version: 1,
    revision: 1,
  }) as ConnectionSummary;
it('uses authoritative creation times, stable unknown order and either live session', () => {
  const connections = [
    connection('a', 'Z', '2026-01-01'),
    connection('b', 'A', '2026-02-01'),
    connection('c', 'Unknown Z', null),
    connection('d', 'Unknown A', null),
  ];
  expect(visibleConnections(connections, [], defaultNavigation, 'en').map((c) => c.id)).toEqual([
    'a',
    'b',
    'c',
    'd',
  ]);
  expect(
    visibleConnections(connections, [], { ...defaultNavigation, direction: 'desc' }, 'en').map(
      (c) => c.id,
    ),
  ).toEqual(['b', 'a', 'c', 'd']);
  const sessions = [
    { connectionId: 'a', phase: 'failed' },
    { connectionId: 'a', phase: 'running' },
  ] as TerminalSessionView[];
  expect(
    visibleConnections(
      connections,
      sessions,
      { ...defaultNavigation, status: 'connected' },
      'en',
    ).map((c) => c.id),
  ).toEqual(['a']);
  expect(
    visibleConnections(
      connections,
      sessions,
      { ...defaultNavigation, status: 'connected', view: 'groups' },
      'en',
    ),
  ).toHaveLength(4);
});

it('ignores retained connection search and filters in groups without mutating them', () => {
  const navigation = {
    ...defaultNavigation,
    view: 'groups' as const,
    search: 'no matching connection',
    status: 'connected' as const,
    direction: 'desc' as const,
  };
  const connections = [connection('one', 'One', null), connection('two', 'Two', null)];
  expect(visibleConnections(connections, [], navigation, 'en')).toHaveLength(2);
  expect(hasSearchOrFilters(navigation)).toBe(false);
  expect(navigation.search).toBe('no matching connection');
  expect(
    visibleConnections(connections, [], { ...navigation, view: 'connections' }, 'en'),
  ).toHaveLength(0);
});
