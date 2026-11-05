import type { ReactNode } from 'react';
import type { DialogSize } from '../components/ui/overlay';

export interface DecisionAction {
  id: string;
  label: string;
  role: 'cancel' | 'primary' | 'secondary';
  tone?: 'default' | 'destructive';
  disabled?: boolean;
  disabledReason?: string;
}
export interface DecisionOptions {
  title: string;
  description: string;
  detail?: ReactNode;
  actions: readonly DecisionAction[];
  tone?: 'neutral' | 'warning' | 'danger';
  size?: DialogSize;
  onAction?(id: string): Promise<boolean>;
}
