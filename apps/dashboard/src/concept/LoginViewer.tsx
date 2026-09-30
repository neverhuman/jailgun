import { useEffect, useRef, useState } from 'react';
import type RFB from '@novnc/novnc';

export function LoginViewer({ accountId, onClose }: { accountId: string; onClose: () => void }) {
  const screen = useRef<HTMLDivElement>(null);
  const connection = useRef<RFB | null>(null);
  const [status, setStatus] = useState('Connecting to your private browser…');
  const [actualSize, setActualSize] = useState(false);
  function toggleSize() {
    const client = connection.current;
    if (!client) return;
    client.scaleViewport = actualSize;
    client.clipViewport = !actualSize;
    client.dragViewport = !actualSize;
    setActualSize(!actualSize);
  }
  useEffect(() => {
    let cancelled = false;
    void import('@novnc/novnc').then(({ default: Client }) => {
      if (cancelled || !screen.current) return;
      const url = new URL(`/api/accounts/${encodeURIComponent(accountId)}/login-view`, window.location.origin);
      url.protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const client = new Client(screen.current, url.toString());
      connection.current = client;
      client.scaleViewport = true;
      client.resizeSession = false;
      client.addEventListener('connect', () => { if (!cancelled) setStatus('Connected. Complete ChatGPT login in the browser below.'); });
      client.addEventListener('disconnect', () => { if (!cancelled) setStatus('Login view closed. Check account status, then reopen the view if login is still active.'); });
      client.addEventListener('securityfailure', () => { if (!cancelled) setStatus('The login view could not connect. Pair the dashboard again and check account status.'); });
      client.addEventListener('credentialsrequired', () => { client.disconnect(); if (!cancelled) setStatus('Unexpected viewer authentication. Close the view and run jailgun doctor --server-browser.'); });
    }).catch(() => { if (!cancelled) setStatus('The login viewer could not load. Reload the dashboard and check the installation.'); });
    return () => { cancelled = true; connection.current?.disconnect(); connection.current = null; };
  }, [accountId]);
  return <section className="loginViewer" aria-label="Private ChatGPT login browser">
    <div className="actions"><button className="secondaryButton" onClick={() => connection.current?.focus()}>Focus login browser</button><button className="secondaryButton" aria-pressed={actualSize} onClick={toggleSize}>{actualSize ? 'Fit browser to view' : 'Use actual size'}</button><button className="quietButton" onClick={onClose}>Close login view</button></div>
    <p role="status">{status}</p>
    <p className="subtle">Enter passwords and verification codes only in this browser. Clipboard sharing is disabled. Use Tab to move through the remote page; Escape lets you return to dashboard controls. The view closes when login ends.</p>
    {actualSize ? <p className="subtle">Drag to pan the browser. Click without dragging to use a control.</p> : null}
    <div ref={screen} className="loginScreen" role="group" aria-label="Remote browser screen" onKeyDownCapture={event => { if (event.key === 'Escape') { event.stopPropagation(); event.currentTarget.parentElement?.querySelector('button')?.focus(); } }} />
  </section>;
}
