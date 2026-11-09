import type { ComponentProps, ReactNode } from 'react';

export function TerminalNoticeOverlay({
  children,
  hidden,
}: {
  children: ReactNode;
  hidden?: boolean;
}) {
  return (
    <div className="terminal-info-overlay" hidden={hidden}>
      {children}
    </div>
  );
}

export function TerminalFloatingNotice({
  children,
  actions,
  dismiss,
  detail,
  ...props
}: Omit<ComponentProps<'aside'>, 'className'> & {
  actions?: ReactNode;
  dismiss?: ReactNode;
  detail?: ReactNode;
}) {
  return (
    <aside {...props} className="terminal-floating-notice" data-dismissible={!!dismiss}>
      <div className="terminal-notice-header">
        <div className="terminal-notice-identity">{children}</div>
        {actions ? <div className="terminal-notice-actions">{actions}</div> : null}
      </div>
      {dismiss ? <div className="terminal-notice-dismiss">{dismiss}</div> : null}
      {detail ? <div className="terminal-notice-body">{detail}</div> : null}
    </aside>
  );
}
