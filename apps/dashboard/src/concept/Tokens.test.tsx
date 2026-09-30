import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { Tokens } from './Tokens';
import { workflowFixtures } from '../generated/workflow-fixtures';
import { validate } from './api';

afterEach(() => vi.unstubAllGlobals());
it('creates an explicitly scoped token, shows its secret once and confirms revocation', async () => {
  const metadata = { account_ids: ['demo-account'], created_ms: 0, id: 'synthetic-token', name: 'Test MCP' };
  const issuedSecret = crypto.randomUUID();
  let issued = false; let revoked = false;
  const fetcher = vi.fn(async (path: string, init?: RequestInit) => {
    if (path === '/api/tokens' && init?.method === 'POST') {
      expect(JSON.parse(String(init.body))).toEqual({ name: 'Test MCP', account_ids: ['demo-account'] });
      issued = true; return new Response(JSON.stringify({ metadata, secret: issuedSecret }));
    }
    if (path === '/api/tokens/synthetic-token') { expect(init?.method).toBe('DELETE'); revoked = true; return new Response(null, { status: 204 }); }
    return new Response(JSON.stringify(issued ? [{ ...metadata, revoked_ms: revoked ? 1 : null }] : []));
  });
  vi.stubGlobal('fetch', fetcher);
  render(<Tokens accounts={workflowFixtures.accounts.map(a => validate('account_readiness', a))} report={() => {}} demo={false} />);
  fireEvent.click(screen.getByText('Automation access for CLI and MCP'));
  fireEvent.change(await screen.findByLabelText('Token name'), { target: { value: 'Test MCP' } });
  expect(screen.getByRole('button', { name: 'Create automation token' })).toBeDisabled();
  fireEvent.click(screen.getByLabelText('demo-account'));
  fireEvent.click(screen.getByRole('button', { name: 'Create automation token' }));
  expect(await screen.findByLabelText('New automation token')).toHaveValue(issuedSecret);
  expect(screen.getByRole('button', { name: 'Create automation token' })).toBeDisabled();
  fireEvent.click(screen.getByRole('button', { name: 'I have saved the token' }));
  expect(screen.queryByLabelText('New automation token')).not.toBeInTheDocument();
  fireEvent.click(await screen.findByRole('button', { name: 'Revoke Test MCP' }));
  expect(revoked).toBe(false);
  fireEvent.click(screen.getByRole('button', { name: 'Keep token' }));
  expect(revoked).toBe(false);
  fireEvent.click(screen.getByRole('button', { name: 'Revoke Test MCP' }));
  fireEvent.click(screen.getByRole('button', { name: 'Confirm revocation' }));
  await waitFor(() => expect(revoked).toBe(true));
  expect(await screen.findByText(/demo-account · Revoked/)).toBeInTheDocument();
  expect(window.localStorage.length).toBe(0);
});
