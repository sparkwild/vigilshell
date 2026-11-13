// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom/vitest';
import { Dialog, DialogTrigger, FormDialog, DialogContent, DialogTitle } from './dialog';
import { Button } from './button';

beforeEach(() => {
  Element.prototype.getAnimations = () => [];
  window.matchMedia = vi.fn(() => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  })) as unknown as typeof window.matchMedia;
});
afterEach(cleanup);

it('gives a modal editor inside a non-modal workspace its own backdrop', () => {
  render(
    <Dialog mode="workspace" open>
      <DialogContent size="editor" closeLabel="Close workspace">
        <DialogTitle>Workspace</DialogTitle>
        <FormDialog open>
          <DialogContent size="form" closeLabel="Close form">
            <DialogTitle>Form</DialogTitle>
          </DialogContent>
        </FormDialog>
      </DialogContent>
    </Dialog>,
  );
  expect(screen.getByRole('dialog', { name: 'Form' })).toBeVisible();
  expect(document.querySelectorAll('[data-slot="dialog-overlay"]')).toHaveLength(1);
});

it('reuses the backdrop when both parent and child are modal', () => {
  render(
    <Dialog open>
      <DialogContent closeLabel="Close parent">
        <DialogTitle>Parent</DialogTitle>
        <FormDialog open>
          <DialogContent closeLabel="Close child">
            <DialogTitle>Child</DialogTitle>
          </DialogContent>
        </FormDialog>
      </DialogContent>
    </Dialog>,
  );
  expect(document.querySelectorAll('[data-slot="dialog-overlay"]')).toHaveLength(1);
});

it('focuses the first editable field, skipping readonly and disabled fields', async () => {
  render(
    <FormDialog open>
      <DialogContent closeLabel="Close">
        <DialogTitle>Edit</DialogTitle>
        <input aria-label="Readonly" readOnly />
        <input aria-label="Disabled" disabled />
        <input aria-label="Name" />
        <button>Save</button>
      </DialogContent>
    </FormDialog>,
  );
  await waitFor(() => expect(screen.getByRole('textbox', { name: 'Name' })).toHaveFocus());
});

it('focuses preview content instead of an action', async () => {
  render(
    <Dialog open>
      <DialogContent purpose="preview" closeLabel="Close">
        <DialogTitle>Preview</DialogTitle>
        <button>Copy</button>
      </DialogContent>
    </Dialog>,
  );
  await waitFor(() => expect(screen.getByRole('dialog', { name: 'Preview' })).toHaveFocus());
});

it('closes through the dedicated view control and restores focus to the trigger', async () => {
  const user = userEvent.setup();
  render(
    <Dialog>
      <DialogTrigger render={<Button />}>Open view</DialogTrigger>
      <DialogContent closeLabel="Close view">
        <DialogTitle>Activity view</DialogTitle>
      </DialogContent>
    </Dialog>,
  );
  const trigger = screen.getByRole('button', { name: 'Open view' });
  await user.click(trigger);
  expect(screen.getByRole('dialog', { name: 'Activity view' })).toBeVisible();
  await user.click(screen.getByRole('button', { name: 'Close view' }));
  await waitFor(() =>
    expect(screen.queryByRole('dialog', { name: 'Activity view' })).not.toBeInTheDocument(),
  );
  expect(trigger).toHaveFocus();
});
