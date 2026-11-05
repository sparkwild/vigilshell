import type {
  ConnectionSummary,
  TerminalSessionView,
  ConnectionGroupView as ConnectionFolder,
} from '@contracts';

import { defaultNavigation as initialNavigation, type NavigationOptions } from './workspace-store';
export type { NavigationOptions } from './workspace-store';
export const defaultNavigation = () => ({ ...initialNavigation });
export function resetFilterSort(options: NavigationOptions): NavigationOptions {
  return { ...defaultNavigation(), view: options.view, search: options.search };
}

// 分组不应用搜索、筛选或排序；连接条件保留，切回连接视图时继续使用。
export function navigationForView(options: NavigationOptions): NavigationOptions {
  return options.view === 'groups'
    ? {
        ...resetFilterSort(options),
        search: '',
        sortBy: 'name',
        direction: 'asc',
      }
    : options;
}

export const hasFilters = (options: NavigationOptions) => options.status !== 'all';
export const hasSearchOrFilters = (options: NavigationOptions) =>
  options.view === 'connections' && (hasFilters(options) || !!options.search.trim());

export function visibleConnections(
  connections: ConnectionSummary[],
  sessions: TerminalSessionView[],
  navigation: NavigationOptions,
  locale: string,
): ConnectionSummary[] {
  const options = navigationForView(navigation);
  const connected = new Set(
    sessions.filter((s) => s.phase === 'running').map((s) => s.connectionId),
  );
  const query = options.search.trim().toLocaleLowerCase(locale);
  const compare = new Intl.Collator(locale, {
    numeric: true,
    sensitivity: 'base',
  }).compare;
  const direction = options.direction === 'asc' ? 1 : -1;

  return connections
    .filter(
      (c) =>
        (options.status === 'all' ||
          (connected.has(c.id) ? 'connected' : 'disconnected') === options.status) &&
        (!query || `${c.name} ${c.host}`.toLocaleLowerCase(locale).includes(query)),
    )
    .sort((a, b) => {
      if (options.sortBy === 'createdAt') {
        // 没有历史创建时间的连接固定置后，不能用名称或更新时间伪造时间顺序。
        if (a.createdAt == null && b.createdAt != null) return 1;
        if (a.createdAt != null && b.createdAt == null) return -1;
        const byTime = (a.createdAt ?? '').localeCompare(b.createdAt ?? '');
        return direction * byTime;
      }
      return direction * compare(a.name, b.name) || a.id.localeCompare(b.id);
    });
}
export interface ConnectionGroup {
  id: string;
  folder?: ConnectionFolder;
  connections: ConnectionSummary[];
}
export function connectionGroups(
  folders: ConnectionFolder[],
  connections: ConnectionSummary[],
  filtered: boolean,
  locale: string,
): ConnectionGroup[] {
  const compare = new Intl.Collator(locale, {
    numeric: true,
    sensitivity: 'base',
  }).compare;
  const groups: ConnectionGroup[] = [...folders]
    .sort(
      (a, b) => a.sortOrder - b.sortOrder || compare(a.name, b.name) || a.id.localeCompare(b.id),
    )
    .map((folder) => ({
      id: 'f:' + folder.id,
      folder,
      connections: connections.filter((c) => c.groupId === folder.id),
    }))
    .filter((group) => !filtered || group.connections.length > 0);
  const unassigned = connections.filter((c) => !c.groupId);
  if (unassigned.length) groups.push({ id: 'u:root', connections: unassigned });
  return groups;
}
