import type { ReactNode, Ref } from 'react';
import { createPortal } from 'react-dom';

export function SettingsFooter({
  feedback,
  children,
  slotRef,
}: {
  feedback: ReactNode;
  children: ReactNode;
  slotRef: Ref<HTMLDivElement>;
}) {
  return (
    <>
      <div className="settings-feedback">{feedback}</div>
      <footer className="settings-footer">
        <div className="settings-footer-default">{children}</div>
        <div className="settings-footer-slot" ref={slotRef} />
      </footer>
    </>
  );
}

export function SettingsFooterContent({
  target,
  children,
}: {
  target: HTMLElement | null;
  children: ReactNode;
}) {
  const content = <div className="settings-footer-content">{children}</div>;
  return target ? createPortal(content, target) : content;
}
