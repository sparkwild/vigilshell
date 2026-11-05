import { WorkspaceFailure } from '../workspace-failure';

import type { ConnectionGroupView, GroupLayout } from '@contracts';

import { type WorkspaceSnapshot, type WorkspaceStore } from '../workspace-store';

type GroupPort = Pick<
  WorkspaceStore,
  | 'run'
  | 'mutation'
  | 'refresh'
  | 'setNavigation'
  | 'setGroupExpanded'
  | 'setGroupEditing'
  | 'getSnapshot'
  | 'feedback'
> & { set(update: Partial<WorkspaceSnapshot>): void };

export class GroupCoordinator {
  constructor(private readonly context: GroupPort) {}

  currentLayout = (): GroupLayout => {
    const groupIds = [...this.context.getSnapshot().groups]
      .sort((a, b) => a.sortOrder - b.sortOrder)
      .map((g) => g.id);
    return {
      revision: this.context.getSnapshot().layoutRevision,
      groupIds,
      buckets: [...groupIds, null].map((groupId) => ({
        groupId,
        connectionIds: this.context
          .getSnapshot()
          .connections.filter((c) => c.groupId === groupId)
          .sort((a, b) => a.groupSortOrder - b.groupSortOrder)
          .map((c) => c.id),
      })),
    };
  };

  commitLayout = async (layout: GroupLayout) => {
    if (this.context.getSnapshot().groupBusy || this.context.feedback.isBusy('group-layout'))
      throw new WorkspaceFailure('busy');
    this.context.set({ groupBusy: true });
    try {
      await this.context.run({
        owner: 'navigation:groups',
        resources: ['group-layout'],
        action: 'nav_dragItem',
        label: 'SSH',
        execute: () => this.context.mutation('updateLayout', layout),
      });
      await this.context.refresh();
    } catch (error) {
      await this.context.refresh().catch(() => undefined);
      throw error;
    } finally {
      this.context.set({ groupBusy: false });
    }
  };

  createGroup = async (surfaceId: string, name: string) => {
    if (this.context.getSnapshot().groupBusy || this.context.feedback.isBusy('group-layout'))
      return;
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
      await this.context.refresh();
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
      await this.context.refresh().catch(() => undefined);
    } finally {
      this.context.set({ groupBusy: false, interactionSurface: null });
    }
  };

  renameGroup = async (group: ConnectionGroupView, draft: string) => {
    if (this.context.getSnapshot().groupBusy || this.context.feedback.isBusy('group-layout'))
      return;
    const name = draft.trim();
    if (!name || name === group.name) {
      this.context.setGroupEditing(null);
      return;
    }
    this.context.set({ groupBusy: true });
    try {
      await this.context.run({
        owner: 'group:' + group.id,
        resources: ['group-layout'],
        action: 'nav_rename',
        label: group.name,
        onResult: () => {
          if (this.context.getSnapshot().groupEditing?.id === group.id)
            this.context.setGroupEditing(null);
        },
        execute: () =>
          this.context.mutation('updateGroup', {
            id: group.id,
            version: group.version,
            name,
          }),
      });
      this.context.setGroupEditing(null);
      await this.context.refresh();
    } catch (error) {
      this.context.setGroupEditing(
        (p) =>
          p && {
            ...p,
            error: error instanceof WorkspaceFailure ? error.code : 'runtime_unavailable',
          },
      );
      await this.context.refresh().catch(() => undefined);
    } finally {
      this.context.set({ groupBusy: false });
    }
  };

  removeGroup = async (group: ConnectionGroupView) => {
    if (this.context.getSnapshot().groupBusy || this.context.feedback.isBusy('group-layout'))
      return;
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
      if (this.context.getSnapshot().groupEditing?.id === group.id)
        this.context.setGroupEditing(null);
      await this.context.refresh();
    } catch {
      await this.context.refresh().catch(() => undefined);
    } finally {
      this.context.set({ groupBusy: false });
    }
  };
}
