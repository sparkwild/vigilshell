import { hasNotificationTarget } from '../features/notifications/target-availability';
import type { NotificationTarget } from '@contracts';
import { NotificationDestination } from '../features/notifications/NotificationDestination';
import { TransferConflictDialog } from '../features/server-tools/TransferConflictDialog';
import { SessionAuthorization } from '../features/mcp/SessionAuthorization';
import { FileEditorsDialog } from '../features/server-tools/FileEditorsDialog';
import type { McpNavigation } from '../../contracts/mcp';
import { AgentPolicyPage } from '../features/mcp/policy/AgentPolicyPage';
import { initialPolicySession } from '../features/mcp/policy/policy-model';
import type { SettingsCategory } from '../features/settings/settings-navigation';
import { subscribeMcpNavigation } from '../features/mcp/model';
import { ConfirmationProvider } from '../shared/ConfirmationProvider';
import { UiProvider } from '../shared/UiProvider';
import { NavigationInteractionProvider } from '../components/ui/navigation-interaction';
import { useWorkspace } from '../features/connections/workspace-context';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Workbench } from './Workbench';
import { SettingsView } from '../features/settings/SettingsView';
import { useDesktop } from './DesktopProvider';
import { usePageNavigation } from './use-page-navigation';

export function App({ fontFailed }: { fontFailed: boolean }) {
  const { store: workspaceStore } = useWorkspace();
  const { t } = useDesktop();
  const { register, registerOverlay, navigate } = usePageNavigation();
  const [notificationNavigation, setNotificationNavigation] = useState(0);
  const [notificationTarget, setNotificationTarget] = useState<NotificationTarget | null>(null);
  const [editorTarget, setEditorTarget] = useState<{ id: string; version: number } | undefined>();
  const [view, setView] = useState<'workbench' | 'settings' | 'activity' | 'policy'>('workbench');
  const [activityTarget, setActivityTarget] = useState<
    Extract<McpNavigation, { view: 'activity' }>
  >({ view: 'activity' });
  const [policyTarget, setPolicyTarget] = useState({
    connectionId: null as string | null,
    version: 0,
  });
  const [policySession, setPolicySession] = useState(initialPolicySession);
  const [settingsRoute, setSettingsRoute] = useState({
    category: 'general' as SettingsCategory,
    version: 0,
    checkUpdates: false,
  });
  const returnFocus = useRef<HTMLElement | null>(null);
  const rememberFocus = useCallback(() => {
    if (view === 'workbench' && document.activeElement instanceof HTMLElement)
      returnFocus.current = document.activeElement;
  }, [view]);
  const restoreWorkbench = useCallback(() => {
    setView('workbench');
    requestAnimationFrame(() => {
      const target = returnFocus.current;
      if (target?.isConnected && !target.closest('[inert]')) target.focus();
      else document.querySelector<HTMLButtonElement>('[data-testid="toggle-left"]')?.focus();
    });
  }, []);
  const restoreHome = useCallback(() => {
    setNotificationTarget(null);
    setEditorTarget(undefined);
    setNotificationNavigation((value) => value + 1);
    for (const id of workspaceStore.getSnapshot().transferWindows)
      workspaceStore.closeTransfers(id);
    workspaceStore.tab('overview');
    restoreWorkbench();
  }, [workspaceStore, restoreWorkbench]);
  const goHome = useCallback(() => {
    void navigate(restoreHome);
  }, [navigate, restoreHome]);
  const goBack = useCallback(() => {
    void navigate(restoreWorkbench);
  }, [navigate, restoreWorkbench]);
  const openSettings = useCallback(
    (section?: 'notifications' | 'about' | 'updates') => {
      void navigate(() => {
        rememberFocus();
        setSettingsRoute((current) => ({
          category:
            section === 'updates'
              ? 'about'
              : section === 'notifications' || section === 'about'
                ? section
                : 'general',
          version: current.version + 1,
          checkUpdates: section === 'updates',
        }));
        setView('settings');
      });
    },
    [navigate, rememberFocus],
  );
  const showAgentTarget = useCallback(
    (target: McpNavigation) => {
      rememberFocus();
      if (target.view === 'activity') {
        setActivityTarget(target);
        setView('activity');
      } else {
        setPolicyTarget((current) => ({
          connectionId: target.connectionId,
          version: current.version + 1,
        }));
        setView('policy');
      }
    },
    [rememberFocus],
  );
  useEffect(
    () =>
      subscribeMcpNavigation((target) => {
        void navigate(() => showAgentTarget(target));
      }),
    [navigate, showAgentTarget],
  );
  useEffect(
    () =>
      window.vigilshell.onTransferNavigation?.((target) => {
        void navigate(() => {
          if (
            !hasNotificationTarget({ kind: 'transfer', ...target }, workspaceStore.getSnapshot())
          ) {
            restoreHome();
            return;
          }
          restoreWorkbench();
          workspaceStore.showTransferNotification(target);
        });
      }),
    [navigate, restoreWorkbench, workspaceStore, restoreHome],
  );
  useEffect(
    () =>
      window.vigilshell.notifications.onNavigate?.((target) => {
        void navigate(async () => {
          if (!hasNotificationTarget(target, workspaceStore.getSnapshot())) {
            restoreHome();
            return;
          }
          try {
            if (target.kind === 'agentTask') {
              const result = await window.vigilshell.mcp.request({
                action: 'task',
                taskId: target.taskId,
              });
              if (
                !result.ok ||
                result.reply?.kind !== 'task' ||
                result.reply.task.id !== target.taskId ||
                result.reply.task.contextId !== target.contextId
              ) {
                restoreHome();
                return;
              }
            }
            if (target.kind === 'fileEditor') {
              const result = await window.vigilshell.fileEditors.request({ action: 'list' });
              const item = result.sessions.find((item) => item.id === target.id);
              if (
                !result.ok ||
                !item ||
                !workspaceStore
                  .getSnapshot()
                  .sessions.some((session) => session.connectionId === item.connectionId)
              ) {
                restoreHome();
                return;
              }
            }
          } catch {
            restoreHome();
            return;
          }
          const current = workspaceStore.getSnapshot();
          if (!hasNotificationTarget(target, current)) {
            restoreHome();
            return;
          }
          setNotificationTarget(null);
          if (target.kind !== 'fileEditor') setNotificationNavigation((value) => value + 1);
          switch (target.kind) {
            case 'agentTask':
              if (target.review) {
                restoreWorkbench();
                workspaceStore.tab(`agent:${target.contextId}`);
              } else
                showAgentTarget({
                  view: 'activity',
                  contextId: target.contextId,
                  taskId: target.taskId,
                });
              break;
            case 'agentContext':
              restoreWorkbench();
              workspaceStore.tab(`agent:${target.contextId}`);
              break;
            case 'agentActivity':
              showAgentTarget({ view: 'activity', contextId: target.contextId });
              break;
            case 'fileEditor':
              setEditorTarget((old) => ({ id: target.id, version: (old?.version ?? 0) + 1 }));
              break;
            case 'runtime':
            case 'mcp':
              setSettingsRoute((old) => ({
                category: target.kind === 'runtime' ? 'general' : 'mcp',
                version: old.version + 1,
                checkUpdates: false,
              }));
              setView('settings');
              break;
            case 'connection':
              if (
                target.runtimeId === current.runtimeId &&
                target.sessionId &&
                current.sessions.some((s) => s.id === target.sessionId)
              ) {
                restoreWorkbench();
                workspaceStore.tab(target.sessionId);
              } else setNotificationTarget(target);
              break;
            case 'transfer':
              restoreWorkbench();
              workspaceStore.showTransferNotification(target);
              break;
            default:
              setNotificationTarget(target);
          }
        });
      }),
    [navigate, workspaceStore, restoreWorkbench, showAgentTarget, restoreHome],
  );
  useEffect(() => {
    const diagnostics = (event: Event) => {
      const id: unknown = (event as CustomEvent<unknown>).detail;
      if (
        typeof id !== 'string' ||
        !workspaceStore.getSnapshot().sessions.some((session) => session.id === id)
      )
        return;
      void navigate(() => {
        restoreWorkbench();
        workspaceStore.showTerminalDiagnostics(id);
      });
    };
    window.addEventListener('vigilshell:terminal-diagnostics', diagnostics);
    return () => {
      window.removeEventListener('vigilshell:terminal-diagnostics', diagnostics);
    };
  }, [navigate, workspaceStore, restoreWorkbench]);
  useEffect(() => window.vigilshell.onOpenSettings(openSettings), [openSettings]);
  useEffect(() => {
    window.vigilshell.rendererReady();
  }, []);
  useEffect(
    () =>
      window.vigilshell.onWorkspaceCommand((command) => {
        if (view === 'workbench') return;
        if (command === 'close') goBack();
        if (command === 'new')
          void navigate(() => {
            restoreWorkbench();
            if (!workspaceStore.getSnapshot().editor) workspaceStore.newConnection();
          });
      }),
    [view, goBack, navigate, restoreWorkbench, workspaceStore],
  );
  return (
    <UiProvider>
      <ConfirmationProvider>
        <SessionAuthorization />
        <div className="relative h-full">
          <FileEditorsDialog
            onUnavailable={goHome}
            notificationTarget={editorTarget}
            navigationVersion={notificationNavigation}
            registerLeaveGuard={registerOverlay}
          />
          {notificationTarget ? (
            <NotificationDestination
              onUnavailable={goHome}
              target={notificationTarget}
              close={() => setNotificationTarget(null)}
            />
          ) : null}
          <TransferConflictDialog />
          <NavigationInteractionProvider>
            <Workbench
              onSettings={openSettings}
              active={view !== 'settings'}
              beforeConnectionAction={() => navigate(restoreWorkbench)}
              policyContent={
                view === 'policy' ? (
                  <AgentPolicyPage
                    target={policyTarget}
                    session={policySession}
                    setSession={setPolicySession}
                    onBack={goBack}
                    register={register}
                  />
                ) : undefined
              }
              activityTarget={view === 'activity' ? activityTarget : undefined}
              onReturnToTerminal={goBack}
            />
          </NavigationInteractionProvider>
          {view === 'settings' ? (
            <SettingsView
              registerExitGuard={register}
              initialCategory={settingsRoute.category}
              navigationVersion={settingsRoute.version}
              checkUpdates={settingsRoute.checkUpdates}
              onBack={goBack}
            />
          ) : null}
          {fontFailed ? (
            <div className="absolute inset-x-6 top-12">
              <Alert>
                <AlertDescription>{t('font_unavailable')}</AlertDescription>
              </Alert>
            </div>
          ) : null}
        </div>
      </ConfirmationProvider>
    </UiProvider>
  );
}
