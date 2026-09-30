import { expect, it, vi } from 'vitest';
import { subscribeEvents } from '../api';
import { MockWebSocket, setupDashboardMocks } from '../App.testSupport';
import { eventFixtures } from '../generated/event-fixtures';
import { EVENT_KINDS } from '../generated/event';

setupDashboardMocks();

it('has a wire fixture for every canonical Rust event kind', () => {
  expect(Object.values(eventFixtures).map(event => event.kind).sort()).toEqual([...EVENT_KINDS].sort());
});

it.each(Object.entries(eventFixtures))('decodes the generated %s contract through the WebSocket consumer', (_name, fixture) => {
  const receive = vi.fn();
  const reject = vi.fn();
  const close = subscribeEvents(receive, { onError: reject });
  try {
    MockWebSocket.instances[0].onmessage?.({ data: JSON.stringify(fixture) });
    expect(reject).not.toHaveBeenCalled();
    expect(receive).toHaveBeenCalledExactlyOnceWith(fixture);
  } finally { close(); }
});
