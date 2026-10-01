import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { LoginViewer } from './LoginViewer';

const clients = vi.hoisted(() => [] as Array<EventTarget & { url: string; disconnect: ReturnType<typeof vi.fn>; focus: ReturnType<typeof vi.fn> }>);
vi.mock('@novnc/novnc', () => ({ default: class extends EventTarget {
  disconnect = vi.fn(); focus = vi.fn();
  constructor(_target: HTMLElement, public url: string) { super(); clients.push(this); }
} }));
afterEach(() => { cleanup(); clients.length = 0; });

it('uses the operator cookie WebSocket and closes the connection on unmount', async () => {
  const close = vi.fn();
  const { unmount } = render(<LoginViewer accountId="synthetic" onClose={close} />);
  await waitFor(() => expect(clients).toHaveLength(1));
  const client = clients[0];
  expect(screen.getByRole('group', { name: 'Remote browser screen' })).toBeInTheDocument();
  const url = new URL(client.url);
  expect(url.pathname).toBe('/api/accounts/synthetic/login-view');
  expect(url.search).toBe(''); expect(url.username).toBe(''); expect(url.password).toBe('');
  client.dispatchEvent(new Event('connect'));
  await screen.findByText('Connected. Complete ChatGPT login in the browser below.');
  fireEvent.click(screen.getByRole('button', { name: 'Use actual size' }));
  expect(client).toMatchObject({ scaleViewport: false, clipViewport: true, dragViewport: true });
  fireEvent.click(screen.getByRole('button', { name: 'Fit browser to view' }));
  expect(client).toMatchObject({ scaleViewport: true, clipViewport: false, dragViewport: false });
  fireEvent.click(screen.getByRole('button', { name: 'Focus login browser' }));
  expect(client.focus).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole('button', { name: 'Close login view' }));
  expect(close).toHaveBeenCalledOnce();
  unmount(); expect(client.disconnect).toHaveBeenCalledOnce();
});

it('reports a rejected connection without collecting viewer credentials', async () => {
  render(<LoginViewer accountId="synthetic" onClose={() => {}} />);
  await waitFor(() => expect(clients).toHaveLength(1));
  clients[0].dispatchEvent(new Event('securityfailure'));
  await screen.findByText(/could not connect. Pair the dashboard again/);
  clients[0].dispatchEvent(new Event('credentialsrequired'));
  await screen.findByText(/Unexpected viewer authentication/);
  expect(clients[0].disconnect).toHaveBeenCalledOnce();
  expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
});
