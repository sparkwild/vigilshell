'use client';

import * as React from 'react';
import { Dialog as DialogPrimitive } from '@base-ui/react/dialog';
import { cn } from 'cn';

import { Button } from '@/components/ui/button';
import { ViewCloseButton } from './view-close-button';
import { overlayClassName, useModalFocus, type DialogPurpose } from './overlay';

const DialogPurposeContext = React.createContext<DialogPurpose>('notice');
const DialogMode = React.createContext<'modal' | 'workspace'>('modal');
const DialogBackdropRequired = React.createContext(false);
function Dialog({
  mode = 'modal',
  ...props
}: Omit<DialogPrimitive.Root.Props, 'modal'> & {
  mode?: 'modal' | 'workspace';
}) {
  const parentMode = React.useContext(DialogMode);
  return (
    <DialogMode value={mode}>
      <DialogBackdropRequired value={parentMode === 'workspace'}>
        <DialogPrimitive.Root data-slot="dialog" modal={mode === 'modal'} {...props} />
      </DialogBackdropRequired>
    </DialogMode>
  );
}

function FormDialog(props: React.ComponentProps<typeof Dialog>) {
  return (
    <DialogPurposeContext value="form">
      <Dialog {...props} disablePointerDismissal />
    </DialogPurposeContext>
  );
}

function DialogForm({ onKeyDownCapture, ...props }: React.ComponentProps<'form'>) {
  return (
    <form
      data-slot="dialog-form"
      {...props}
      onKeyDownCapture={(event) => {
        if (event.key === 'Enter' && (event.nativeEvent.isComposing || event.keyCode === 229))
          event.preventDefault();
        onKeyDownCapture?.(event);
      }}
    />
  );
}

function DialogTrigger({ ...props }: DialogPrimitive.Trigger.Props) {
  return <DialogPrimitive.Trigger data-slot="dialog-trigger" {...props} />;
}

function DialogPortal({ ...props }: DialogPrimitive.Portal.Props) {
  return <DialogPrimitive.Portal data-slot="dialog-portal" {...props} />;
}

function DialogClose({ ...props }: DialogPrimitive.Close.Props) {
  return <DialogPrimitive.Close data-slot="dialog-close" {...props} />;
}

function DialogOverlay({ className, ...props }: DialogPrimitive.Backdrop.Props) {
  return (
    <DialogPrimitive.Backdrop
      data-slot="dialog-overlay"
      className={cn(overlayClassName, className)}
      {...props}
    />
  );
}

function DialogContent({
  ref,
  purpose: requestedPurpose,
  className,
  children,
  showCloseButton = true,
  size = 'default',
  closeLabel,
  ...props
}: DialogPrimitive.Popup.Props & {
  purpose?: DialogPurpose;
  size?: 'default' | 'form' | 'wide' | 'editor' | 'transfer';
} & (
    { showCloseButton?: true; closeLabel: string } | { showCloseButton: false; closeLabel?: string }
  )) {
  const defaultPurpose = React.useContext(DialogPurposeContext);
  const purpose = requestedPurpose ?? defaultPurpose;
  const focus = useModalFocus(purpose, ref);
  const mode = React.useContext(DialogMode);
  const forceBackdrop = React.useContext(DialogBackdropRequired);
  return (
    <DialogPortal>
      {/* Base UI 默认隐藏嵌套遮罩；非模态父工作窗没有可复用的遮罩。 */}
      {mode === 'modal' ? <DialogOverlay forceRender={forceBackdrop} /> : null}
      <DialogPrimitive.Popup
        data-slot="dialog-content"
        data-purpose={purpose}
        ref={focus.ref}
        initialFocus={focus.initialFocus}
        tabIndex={-1}
        data-size={size}
        data-mode={mode}
        className={cn(
          'fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 grid w-full max-w-[calc(100%-2rem)] max-h-[calc(100dvh-4rem)] overflow-y-auto gap-4 rounded-xl bg-popover p-4 text-sm text-popover-foreground ring-1 ring-foreground/10 duration-100 outline-none data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95',
          mode === 'workspace' ? 'z-40' : 'z-50',
          className,
        )}
        {...props}
      >
        {children}
        {showCloseButton && (
          <DialogPrimitive.Close
            data-slot="dialog-close"
            render={<ViewCloseButton className="absolute top-2 right-2" label={closeLabel!} />}
          />
        )}
      </DialogPrimitive.Popup>
    </DialogPortal>
  );
}

function DialogHeader({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div data-slot="dialog-header" className={cn('flex flex-col gap-2', className)} {...props} />
  );
}

function DialogFooter({
  className,
  closeLabel,
  children,
  ...props
}: React.ComponentProps<'div'> & {
  closeLabel?: string;
}) {
  return (
    <div
      data-slot="dialog-footer"
      className={cn(
        '-mx-4 -mb-4 flex flex-col-reverse gap-2 rounded-b-xl border-t bg-muted/50 p-4 sm:flex-row sm:justify-end',
        className,
      )}
      {...props}
    >
      {children}
      {closeLabel && (
        <DialogPrimitive.Close render={<Button variant="outline" />}>
          {closeLabel}
        </DialogPrimitive.Close>
      )}
    </div>
  );
}

function DialogTitle({ className, ...props }: DialogPrimitive.Title.Props) {
  return (
    <DialogPrimitive.Title
      data-slot="dialog-title"
      className={cn('text-base leading-none font-medium', className)}
      {...props}
    />
  );
}

function DialogDescription({ className, ...props }: DialogPrimitive.Description.Props) {
  return (
    <DialogPrimitive.Description
      data-slot="dialog-description"
      className={cn(
        'text-sm text-muted-foreground *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground',
        className,
      )}
      {...props}
    />
  );
}

function DialogBody({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="dialog-body"
      className={cn(
        'min-h-0 min-w-0 overflow-auto empty:hidden [overflow-wrap:anywhere]',
        className,
      )}
      {...props}
    />
  );
}

export {
  DialogBody,
  Dialog,
  FormDialog,
  DialogForm,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
};
