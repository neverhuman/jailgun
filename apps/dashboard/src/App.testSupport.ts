import { afterEach, beforeEach, vi } from 'vitest';

import { fixtureRuns } from './fixtures';
import type { JailgunEvent } from './types';

export class MockWebSocket {
  static instances: MockWebSocket[] = [];
  static OPEN = 1;
  readyState = MockWebSocket.OPEN;
  onmessage: ((message: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(public url: string) {
    MockWebSocket.instances.push(this);
  }

  close = vi.fn();

  emit(event: JailgunEvent) {
    this.onmessage?.({ data: JSON.stringify(event) });
  }
}

export function setupDashboardMocks(): void {
  beforeEach(() => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string) => {
        if (url === '/api/runs') {
          return jsonResponse(fixtureRuns);
        }
        if (url.startsWith('/api/receipts/')) {
          return jsonResponse({ run_id: 'fixture-run', receipts: [{ tab_id: 1, sha256: 'abc123' }] });
        }
        if (url === '/api/history') {
          return jsonResponse([]);
        }
        return { ok: false, status: 404, json: async () => ({}) };
      })
    );
    MockWebSocket.instances = [];
    vi.stubGlobal('WebSocket', MockWebSocket);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });
}

export function jsonResponse(body: unknown) {
  return {
    ok: true,
    status: 200,
    json: async () => body
  };
}
