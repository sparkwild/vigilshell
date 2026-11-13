import { UpdateButton } from '../features/settings/UpdateIndicators';
import { AlertCircleIcon } from 'lucide-react';
import { useUiStateErrors } from '../shared/platform/ui-state-store';
import { useMcp } from '../features/mcp/model';
import { useNotificationFeedback } from '../features/notifications/NotificationFeedbackProvider';
import { needsAttention } from '../features/notifications/FeatureFeedback';
import { navigateMcp } from '../features/mcp/model';
import { ShieldCheckIcon, HistoryIcon } from 'lucide-react';
import { ConnectionNavigation } from '../features/connections/catalogue-view';
import { SettingsIcon } from 'lucide-react';
import {
  SidebarContent,
  SidebarFooter,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '../components/ui/sidebar';

import { useDesktop } from './DesktopProvider';
export function WorkbenchNavigation({
  onSettings,
  onConnected,
  activityActive = false,
  policyActive = false,
}: {
  onSettings(): void;
  onConnected(): void;
  activityActive?: boolean;
  policyActive?: boolean;
}) {
  const { t } = useDesktop();
  const mcp = useMcp();
  const scopes = useUiStateErrors();
  const notices = useNotificationFeedback().filter(needsAttention);
  const agentIssue = notices.some((item) =>
    ['agentTask', 'agentContext', 'agentActivity'].includes(item.target.kind),
  );
  const serviceIssue = !!mcp.error || !!mcp.snapshot?.error || scopes.includes('pickers');
  return (
    <>
      <SidebarContent className="px-3 py-3">
        <ConnectionNavigation
          onConnected={onConnected}
          showSelection={!activityActive && !policyActive}
        />
      </SidebarContent>
      <SidebarFooter className="shrink-0 p-3">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              isActive={policyActive}
              onClick={() => navigateMcp({ view: 'policy', connectionId: null })}
            >
              <ShieldCheckIcon />
              <span>{t('agent_policy_navigation')}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarMenuButton
              isActive={activityActive}
              onClick={() => navigateMcp({ view: 'activity' })}
            >
              {agentIssue ? (
                <AlertCircleIcon aria-label={t('feedback_attention')} />
              ) : (
                <HistoryIcon />
              )}
              <span>{t('mcp_activity')}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
          <SidebarMenuItem>
            <div className="flex min-w-0 items-center gap-1">
              <SidebarMenuButton
                className="min-w-0 flex-1"
                onClick={onSettings}
                data-testid="open-settings"
              >
                {serviceIssue ? (
                  <AlertCircleIcon aria-label={t('feedback_attention')} />
                ) : (
                  <SettingsIcon />
                )}
                <span>{t('settings')}</span>
              </SidebarMenuButton>
              <UpdateButton />
            </div>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>
    </>
  );
}
