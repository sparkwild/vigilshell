import { WorkspaceFailure } from '../workspace-failure';

import type { ConnectionGroupView, GroupLayout } from '@contracts';

import { type WorkspaceSnapshot, type WorkspaceStore } from '../workspace-store';

type GroupPort = Pick<
  WorkspaceStore,
  | 'run'
  | 'mutation'
  | 'refresh'
  | 'refreshGroups'
  | 'acceptGroupRename'
  | 'setNavigation'
  | 'setGroupExpanded'
  | 'setGroupEditing'
  | 'getSnapshot'
  | 'feedback'
> & { set(update: Partial<WorkspaceSnapshot>): void };

export class GroupCoordinator {
  constructor(private readonly context: GroupPort) {}

  currentLayout = (): GroupLayout => {
    const { groups, connections, unavailableConnections, layoutRevision } =
      this.context.getSnapshot();
    const groupIds = [...groups].sort((a, b) => a.sortOrder - b.sortOrder).map((g) => g.id);
    return {
      revision: layoutRevision,
      groupIds,
      buckets: [...groupIds, null].map((groupId) => ({
        groupId,
        connectionIds: [...connections, ...unavailableConnections]
          .filter((c) => c.groupId === groupId)
          .sort((a, b) => a.groupSortOrder - b.groupSortOrder)
          .map((c) => c.id),
      })),
    };
  };

  commitLayout = async (layout: GroupLayout) => {
    if (this.context.getSnapshot().groupBusy || this.context.feedback.isBusy('group-layout'))
      throw new WorkspaceFailure('busy');
    this.context.set({ groupBusy: true });
    const runtime = this.context.getSnapshot().runtimeId;
    try {
      await this.context.run({
        owner: 'navigation:groups',
        resources: ['group-layout'],
        action: 'nav_dragItem',
        label: 'SSH',
        execute: () => this.context.mutation('updateLayout', layout),
      });
      if (runtime === this.context.getSnapshot().runtimeId) await this.context.refresh();
    } catch (error) {
      if (runtime === this.context.getSnapshot().runtimeId)
        await this.context.refresh().catch(() => undefined);
      throw error;
    } finally {
      if (runtime === this.context.getSnapshot().runtimeId) this.context.set({ groupBusy: false });
    }
  };

  createGroup = async (surfaceId: string, name: string) => {
    if (this.context.getSnapshot().groupBusy || this.context.feedback.isBusy('group-layout'))
      return;
    const runtime = this.context.getSnapshot().runtimeId;
    this.context.set({ groupBusy: true, interactionSurface: surfaceId });
    this.context.setNavigation((p) => ({ ...p, view: 'groups' }));
    try {
      const result = await this.context.run({
        owner: 'navigation:groups',
        resources: ['group-layout'],
        action: 'nav_newFolder',
        label: name,
        execute: () => this.context.mutation('createGroup', { name }),
      });
      if (runtime !== this.context.getSnapshot().runtimeId) return;
      await this.context.refresh();
      if (runtime !== this.context.getSnapshot().runtimeId) return;
      if (result.kind !== 'group') throw new WorkspaceFailure('protocol_mismatch');
      const group = result.group;
      this.context.setGroupExpanded((p) => new Set(p).add('f:' + group.id));
      this.context.setGroupEditing({
        id: group.id,
        surfaceId,
        draft: group.name,
        error: null,
      });
    } catch {
      if (runtime === this.context.getSnapshot().runtimeId)
        await this.context.refresh().catch(() => undefined);
    } finally {
      if (runtime === this.context.getSnapshot().runtimeId)
        this.context.set({ groupBusy: false, interactionSurface: null });
    }
  };

  renameGroup = async (group: ConnectionGroupView, draft: string) => {
    const { runtimeId, groupsReady, groupEditing } = this.context.getSnapshot();
    let editing = groupEditing;
    const owner = 'group:' + group.id;
    if ((runtimeId && !groupsReady) || this.context.feedback.isBusy(owner)) return;
    const ownsDraft = () =>
      this.context.getSnapshot().runtimeId === runtimeId &&
      this.context.getSnapshot().groupEditing === editing &&
      editing?.id === group.id;
    const name = draft.trim();
    if (!name || name === group.name) {
      if (ownsDraft()) this.context.setGroupEditing(null);
      return;
    }
    try {
      await this.context.run({
        owner,
        action: 'nav_rename',
        label: group.name,
        onResult: (result) => {
          if (result.kind !== 'group') throw new WorkspaceFailure('protocol_mismatch');
          this.context.acceptGroupRename(result.group);
          if (ownsDraft()) this.context.setGroupEditing(null);
        },
        execute: () =>
          this.context.mutation('updateGroup', {
            id: group.id,
            version: group.version,
            name,
          }),
      });
    } catch (error) {
      if (ownsDraft() && editing) {
        editing = {
          ...editing,
          error: error instanceof WorkspaceFailure ? error.code : 'runtime_unavailable',
        };
        this.context.setGroupEditing(editing);
      }
      if (
        runtimeId === this.context.getSnapshot().runtimeId &&
        error instanceof WorkspaceFailure &&
        ['conflict', 'not_found'].includes(error.code)
      )
        await this.context.refreshGroups();
    }
  };

  removeGroup = async (group: ConnectionGroupView) => {
    if (
      this.context.getSnapshot().groupBusy ||
      this.context.feedback.isBusy('group-layout', 'group:' + group.id)
    )
      return;
    const runtime = this.context.getSnapshot().runtimeId;
    this.context.set({ groupBusy: true });
    try {
      await this.context.run({
        owner: 'group:' + group.id,
        resources: ['group-layout'],
        action: 'nav_removeGroup',
        label: group.name,
        execute: () =>
          this.context.mutation('deleteGroup', {
            id: group.id,
            version: group.version,
          }),
      });
      if (runtime !== this.context.getSnapshot().runtimeId) return;
      if (this.context.getSnapshot().groupEditing?.id === group.id)
        this.context.setGroupEditing(null);
      await this.context.refresh();
    } catch {
      if (runtime === this.context.getSnapshot().runtimeId)
        await this.context.refresh().catch(() => undefined);
    } finally {
      if (runtime === this.context.getSnapshot().runtimeId) this.context.set({ groupBusy: false });
    }
  };
}
