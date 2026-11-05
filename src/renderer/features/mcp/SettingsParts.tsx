import type { ReactNode } from 'react';
import { Card, CardContent } from '../../components/ui/card';
import { FieldGroup } from '../../components/ui/field';

export type LeaveGuard = (reason?: 'switch') => Promise<boolean>;
export type RegisterLeaveGuard = (guard: LeaveGuard | null) => void;
export function SettingsGroup({ children }: { children: ReactNode }) {
  return (
    <Card appearance="settings">
      <CardContent className="p-0">
        <FieldGroup className="gap-0">{children}</FieldGroup>
      </CardContent>
    </Card>
  );
}
