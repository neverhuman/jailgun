/** Generated from Rust schemas by scripts/generate-contracts.mjs. DO NOT EDIT. */
import type { WorkflowContracts } from './workflow';
declare const validators: { [K in keyof WorkflowContracts]: (value: unknown) => value is WorkflowContracts[K] };
export = validators;
