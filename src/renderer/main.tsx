import { startTransferFeedback } from './features/notifications/transfer-feedback-runtime';
import { FileEditorsStore } from './features/server-tools/file-editors-store';
import { FileEditorsProvider } from './features/server-tools/FileEditorsProvider';
import { NotificationFeedbackStore } from './features/notifications/feedback-store';
import { NotificationFeedbackProvider } from './features/notifications/NotificationFeedbackProvider';
import { WorkspaceProvider } from './features/connections/workspace-context';
import { WorkspaceStore } from './features/connections/workspace-store';
import { createRoot } from 'react-dom/client';
import { createDesktopStore } from './shared/platform/desktop-store';
import { DesktopProvider, applyDisplay } from './app/DesktopProvider';
import { App } from './app/App';
import './styles/index.css';
const feedback = new NotificationFeedbackStore(window.vigilshell.notifications);
feedback.start();
const editors = new FileEditorsStore(window.vigilshell.fileEditors);
editors.start();
const workspace = new WorkspaceStore();
const stopTransferFeedback = startTransferFeedback(workspace);
const store = createDesktopStore(window.vigilshell);
await store.initialize();
applyDisplay(store.getSnapshot().appearance);
const loaded = Promise.all([
  document.fonts.load('400 13px "Inter Variable"'),
  document.fonts.load('400 13px "Noto Sans SC Variable"', '设置'),
]);
let timeout: ReturnType<typeof setTimeout> | undefined;
let fontFailed = false;
try {
  await Promise.race([
    loaded,
    new Promise((_, reject) => {
      timeout = setTimeout(() => reject(Error('font_timeout')), 3000);
    }),
  ]);
} catch {
  fontFailed = true;
  console.warn('bundled_font_unavailable');
} finally {
  clearTimeout(timeout);
}
createRoot(document.getElementById('root')!).render(
  <DesktopProvider store={store}>
    <WorkspaceProvider store={workspace}>
      <NotificationFeedbackProvider store={feedback}>
        <FileEditorsProvider store={editors}>
          <App fontFailed={fontFailed} />
        </FileEditorsProvider>
      </NotificationFeedbackProvider>
    </WorkspaceProvider>
  </DesktopProvider>,
);
window.addEventListener(
  'pagehide',
  () => {
    stopTransferFeedback();
    feedback.dispose();
    editors.dispose();
    store.dispose();
    workspace.dispose();
  },
  { once: true },
);
