import { ArchiveDashboard } from './ArchiveDashboard';
import { ConceptDashboard } from './concept/ConceptDashboard';
import type { DataSource } from './useDashboardData';

export function App({ mode = 'api' }: { mode?: DataSource }) {
  return new URLSearchParams(window.location.search).get('advanced') === '1'
    ? <ArchiveDashboard mode={mode} /> : <ConceptDashboard demo={mode === 'fixture'} />;
}
