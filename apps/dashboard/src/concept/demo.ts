import { workflowFixtures } from '../generated/workflow-fixtures';
import { validate } from './api';
import type { ConceptSnapshot } from './useConceptData';

export const demoData: ConceptSnapshot = {
  accounts: workflowFixtures.accounts.map(value => validate('account_readiness', value)),
  sessions: [validate('account_session', workflowFixtures['account-ready'])],
  runs: [validate('run', workflowFixtures['run-completed'])],
  attempts: workflowFixtures['completed-attempts'].map(value => validate('attempt', value)),
  events: workflowFixtures.events.map(value => validate('event', value))
};
export const demoArtifacts: Record<string, string> = workflowFixtures.artifacts;
