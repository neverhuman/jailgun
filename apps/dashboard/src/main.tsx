import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import './styles.css';
import './concept/styles.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App mode={new URLSearchParams(window.location.search).get('demo') === '1' ? 'fixture' : 'api'} />
  </StrictMode>
);

