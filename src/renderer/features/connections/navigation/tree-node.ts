import type { ConnectionSummary, ConnectionGroupView as ConnectionFolder } from '@contracts';

export interface Node {
  id: string;
  name: string;
  kind: 'folder' | 'connection' | 'catalog' | 'notice' | 'more' | 'emptyGroup' | 'groupEnd';
  children: Node[];
  connection?: ConnectionSummary;
  folder?: ConnectionFolder;
  parentConnection?: string;
  offset?: number;
  groupId?: string | null;
  lastInGroup?: boolean;
}
