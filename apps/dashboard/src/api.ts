import { fixtureEvents, fixtureReceipts, fixtureRuns } from './fixtures';
import type { JailgunEvent, ReceiptResponse, RunSnapshot } from './types';

export async function fetchRuns(fetcher: typeof fetch = fetch): Promise<RunSnapshot[]> {
  try {
    const response = await fetcher('/api/runs');
    if (!response.ok) {
      throw new Error(`GET /api/runs failed ${response.status}`);
    }
    return (await response.json()) as RunSnapshot[];
  } catch {
    return fixtureRuns;
  }
}

export async function fetchReceipts(runId: string, fetcher: typeof fetch = fetch): Promise<ReceiptResponse> {
  try {
    const response = await fetcher(`/api/receipts/${encodeURIComponent(runId)}`);
    if (!response.ok) {
      throw new Error(`GET /api/receipts failed ${response.status}`);
    }
    return (await response.json()) as ReceiptResponse;
  } catch {
    return { ...fixtureReceipts, run_id: runId };
  }
}

export function subscribeEvents(onEvent: (event: JailgunEvent) => void): () => void {
  if (typeof WebSocket === 'undefined') {
    fixtureEvents.forEach(onEvent);
    return () => undefined;
  }
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const url = `${protocol}//${window.location.host}/ws/events`;
  let socket: WebSocket | null = null;
  try {
    socket = new WebSocket(url);
  } catch {
    fixtureEvents.forEach(onEvent);
    return () => undefined;
  }
  const fallback = window.setTimeout(() => {
    if (socket?.readyState !== WebSocket.OPEN) {
      fixtureEvents.forEach(onEvent);
    }
  }, 250);
  socket.onmessage = (message) => {
    try {
      onEvent(JSON.parse(message.data) as JailgunEvent);
    } catch {
      // Ignore malformed event frames; the server owns validation.
    }
  };
  socket.onerror = () => {
    fixtureEvents.forEach(onEvent);
  };
  return () => {
    window.clearTimeout(fallback);
    socket?.close();
  };
}

