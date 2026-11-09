import { InfoIcon, TriangleAlertIcon } from 'lucide-react';
import { useRef, useLayoutEffect, type ReactNode } from 'react';
import { DialogBody } from '../components/ui/dialog';
import { CodeDetails } from './CodeDetails';
import type { DialogSize } from '../components/ui/overlay';
import { Button } from '../components/ui/button';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
} from '../components/ui/alert-dialog';

export function ConfirmationDialog({
  open,
  focusToken,
  purpose = 'confirm',
  title,
  description,
  detail,
  cancelLabel,
  cancelPlacement = 'end',
  busy = false,
  tone = 'warning',
  onCancel,
  children,
  size = 'default',
  onClosed,
}: {
  open: boolean;
  focusToken?: object | null;
  purpose?: 'confirm' | 'preview';
  title: string;
  description: string;
  detail?: ReactNode;
  cancelLabel: string;
  cancelPlacement?: 'start' | 'end';
  busy?: boolean;
  tone?: 'neutral' | 'warning' | 'danger';
  onCancel(): void;
  children: ReactNode;
  size?: DialogSize;
  onClosed?(): void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
  const content = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (open && focusToken) (purpose === 'preview' ? content.current : cancel.current)?.focus();
  }, [open, focusToken, purpose]);
  return (
    <AlertDialog
      open={open}
      onOpenChangeComplete={(next) => {
        if (!next) onClosed?.();
      }}
      onOpenChange={(next) => {
        if (!next && !busy) onCancel();
      }}
    >
      <AlertDialogContent
        ref={content}
        tabIndex={-1}
        initialFocus={purpose === 'preview' ? content : cancel}
        size={size}
      >
        <AlertDialogHeader>
          <AlertDialogMedia tone={tone} aria-hidden="true">
            {tone === 'neutral' ? <InfoIcon /> : <TriangleAlertIcon />}
          </AlertDialogMedia>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        {detail ? (
          <DialogBody>
            {typeof detail === 'string' ? <CodeDetails>{detail}</CodeDetails> : detail}
          </DialogBody>
        ) : null}
        <AlertDialogFooter>
          <Button
            ref={cancel}
            className={cancelPlacement === 'start' ? 'sm:mr-auto' : undefined}
            type="button"
            variant="outline"
            disabled={busy}
            onClick={onCancel}
          >
            {cancelLabel}
          </Button>
          {children}
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
