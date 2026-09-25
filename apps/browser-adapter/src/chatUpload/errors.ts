import type { SendButtonObservation } from './types';

export class MissingChatControlError extends Error {
  constructor(controlName: string) {
    super(`missing chat control: ${controlName}`);
    this.name = 'MissingChatControlError';
  }
}

export class PromptSubmitReadinessError extends Error {
  readonly lastObserved: SendButtonObservation | null;

  constructor(message: string, lastObserved: SendButtonObservation | null) {
    super(message);
    this.name = 'PromptSubmitReadinessError';
    this.lastObserved = lastObserved;
  }
}
