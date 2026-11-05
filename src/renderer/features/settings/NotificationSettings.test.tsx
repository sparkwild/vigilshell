// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { act, cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { NotificationSettings } from './NotificationSettings';
import { defaultPreferences } from '../../../shared/preferences';
import type {
  NotificationAuthorization,
  SystemNotificationStatus,
} from '../../../contracts/system-notifications';
import { notificationApiFixture } from '../../../../tests/fixtures/notifications';
afterEach(cleanup);
it('shows categories but no permission warning or settings action when authorized', async () => {
  const api = notificationApiFixture();
  api.openSystemSettings = vi.fn(api.openSystemSettings);
  render(
    <NotificationSettings
      api={api}
      preferences={defaultPreferences}
      disabled={false}
      locale="zh-CN"
      onUpdate={vi.fn()}
    />,
  );
  await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
  expect(screen.queryByRole('status')).not.toBeInTheDocument();
  expect(screen.getAllByRole('switch')).toHaveLength(2);
  expect(screen.queryByRole('switch', { name: '启用任务完成通知' })).not.toBeInTheDocument();
  expect(screen.getByRole('switch', { name: '启用待处理通知' })).toBeChecked();
  expect(
    screen.getByText('控制下方各类系统通知的发送时机；选择“关闭”后不发送系统通知'),
  ).toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '发送测试通知' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '允许系统通知' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: '前往通知设置' })).not.toBeInTheDocument();
  expect(api.openSystemSettings).not.toHaveBeenCalled();
});
it('retains category values while the global mode is off and permissions are denied', async () => {
  const api = notificationApiFixture();
  api.getStatus = async () => ({ authorization: 'denied', version: 1 });
  render(
    <NotificationSettings
      api={api}
      preferences={{
        ...defaultPreferences,
        systemNotificationMode: 'off',
        executionIssueNotificationEnabled: false,
      }}
      disabled={false}
      locale="zh-CN"
      onUpdate={vi.fn()}
    />,
  );
  for (const control of screen.getAllByRole('switch'))
    expect(control).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getByRole('switch', { name: '启用执行异常通知' })).not.toBeChecked();
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('系统通知未开启'));
  expect(
    within(screen.getByRole('alert')).getByRole('button', { name: '前往通知设置' }),
  ).toBeEnabled();
  expect(screen.getByRole('combobox')).toBeEnabled();
});
it('saves individual category updates through the existing preference path', async () => {
  const save = vi.fn().mockResolvedValue(undefined);
  render(
    <NotificationSettings
      api={notificationApiFixture()}
      preferences={defaultPreferences}
      disabled={false}
      locale="zh-CN"
      onUpdate={save}
    />,
  );
  await userEvent.click(screen.getByRole('switch', { name: '启用执行异常通知' }));
  expect(save).toHaveBeenCalledWith({ executionIssueNotificationEnabled: false });
});

const unauthorizedStates: NotificationAuthorization[] = [
  'notDetermined',
  'denied',
  'provisional',
  'unknown',
  'unsupported',
];
it.each(unauthorizedStates)(
  'uses the same actionable warning for %s without locking preferences',
  async (authorization) => {
    const api = notificationApiFixture();
    api.getStatus = vi.fn(async () => ({ authorization, version: 1 }));
    api.openSystemSettings = vi.fn(api.openSystemSettings);
    const save = vi.fn();
    render(
      <NotificationSettings
        api={api}
        preferences={defaultPreferences}
        disabled={false}
        locale="zh-CN"
        onUpdate={save}
      />,
    );
    await waitFor(() => expect(api.getStatus).toHaveResolved());
    const warning = screen.getByRole('alert');
    expect(warning).toHaveAttribute('data-variant', 'warning');
    expect(warning).toHaveTextContent('系统通知未开启');
    expect(warning).toHaveTextContent(
      '允许通知后，应用才能在需要处理或执行异常时，通过系统通知提醒你。',
    );
    expect(screen.getAllByRole('button', { name: '前往通知设置' })).toHaveLength(1);
    expect(within(warning).getByRole('button', { name: '前往通知设置' })).toBeEnabled();
    expect(api.openSystemSettings).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('switch', { name: '启用待处理通知' }));
    expect(save).toHaveBeenCalledWith({ attentionRequiredNotificationEnabled: false });
  },
);
it('refreshes permission on focus, recovers after query failure, and ignores old broadcasts', async () => {
  const api = notificationApiFixture();
  let publish!: (status: SystemNotificationStatus) => void;
  const unsubscribe = vi.fn();
  api.onStatusChanged = (listener) => {
    publish = listener;
    return unsubscribe;
  };
  const getStatus = vi.fn(api.getStatus);
  api.getStatus = getStatus;
  const { unmount } = render(
    <NotificationSettings
      api={api}
      preferences={defaultPreferences}
      disabled={false}
      locale="zh-CN"
      onUpdate={vi.fn()}
    />,
  );
  await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
  getStatus.mockRejectedValueOnce(new Error('unavailable'));
  act(() => {
    window.dispatchEvent(new Event('focus'));
  });
  await screen.findByRole('alert');
  expect(screen.queryByRole('status')).not.toBeInTheDocument();
  getStatus.mockResolvedValueOnce({ authorization: 'authorized', version: 3 });
  act(() => {
    window.dispatchEvent(new Event('focus'));
  });
  await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
  expect(screen.queryByRole('button', { name: '前往通知设置' })).not.toBeInTheDocument();
  act(() => {
    publish({ authorization: 'denied', version: 2 });
  });
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  act(() => {
    publish({ authorization: 'denied', version: 4 });
  });
  expect(screen.getByRole('alert')).toHaveTextContent('系统通知未开启');
  unmount();
  expect(unsubscribe).toHaveBeenCalledOnce();
  window.dispatchEvent(new Event('focus'));
  expect(getStatus).toHaveBeenCalledTimes(3);
});
it('does not let an outdated query failure override a new authorization broadcast', async () => {
  const api = notificationApiFixture();
  let reject!: (reason: Error) => void;
  api.getStatus = () =>
    new Promise((_resolve, fail) => {
      reject = fail;
    });
  let publish!: (status: SystemNotificationStatus) => void;
  api.onStatusChanged = (listener) => {
    publish = listener;
    return () => {};
  };
  render(
    <NotificationSettings
      api={api}
      preferences={defaultPreferences}
      disabled={false}
      locale="zh-CN"
      onUpdate={vi.fn()}
    />,
  );
  expect(screen.getByRole('alert')).toHaveTextContent('系统通知未开启');
  await act(async () => {
    publish({ authorization: 'authorized', version: 2 });
    reject(new Error('old query'));
  });
  expect(screen.queryByRole('alert')).not.toBeInTheDocument();
});
it('reports settings-open failures beside the entry and clears them after a successful retry', async () => {
  const api = notificationApiFixture();
  api.getStatus = async () => ({ authorization: 'denied', version: 1 });
  const open = vi.fn(api.openSystemSettings).mockRejectedValueOnce(new Error('open failed'));
  api.openSystemSettings = open;
  render(
    <NotificationSettings
      api={api}
      preferences={defaultPreferences}
      disabled={false}
      locale="zh-CN"
      onUpdate={vi.fn()}
    />,
  );
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('系统通知未开启'));
  await userEvent.click(screen.getByRole('button', { name: '前往通知设置' }));
  expect(screen.getByRole('status')).toHaveTextContent('未能打开系统通知设置，请重试');
  expect(
    within(screen.getByRole('alert')).getByRole('button', { name: '前往通知设置' }),
  ).toBeEnabled();
  await userEvent.click(screen.getByRole('button', { name: '前往通知设置' }));
  expect(screen.queryByRole('status')).not.toBeInTheDocument();
  expect(open).toHaveBeenCalledTimes(2);
});
it('keeps the warning after opening settings until permission actually changes and localizes English', async () => {
  const api = notificationApiFixture();
  api.getStatus = async () => ({ authorization: 'denied', version: 1 });
  render(
    <NotificationSettings
      api={api}
      preferences={defaultPreferences}
      disabled={false}
      locale="en"
      onUpdate={vi.fn()}
    />,
  );
  await userEvent.click(screen.getByRole('button', { name: 'Go to notification settings' }));
  expect(screen.getByRole('alert')).toHaveTextContent('System notifications are not enabled');
  expect(
    screen.queryByRole('switch', { name: 'Enable task completion notifications' }),
  ).not.toBeInTheDocument();
  expect(
    screen.getByRole('switch', { name: 'Enable action required notifications' }),
  ).toBeChecked();
  expect(
    screen.getByRole('switch', { name: 'Enable execution issue notifications' }),
  ).toBeChecked();
  expect(screen.queryByRole('status')).not.toBeInTheDocument();
});
