// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';
import { Dialog, FormDialog, DialogContent, DialogTitle } from './dialog';

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
