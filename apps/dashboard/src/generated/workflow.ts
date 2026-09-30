/** Generated from Rust by scripts/generate-contracts.mjs. DO NOT EDIT. */

/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ModelSelection".
 */
export type ModelSelection =
  | {
      mode: "current";
    }
  | {
      mode: "specific";
      name: string;
    };
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ServiceState".
 */
export type ServiceState = "running" | "stopping" | "stopped";

export interface WorkflowContracts {
  account_readiness: AccountReadiness;
  account_session: AccountSession;
  artifact_chunk: ArtifactChunk;
  artifact_read: ArtifactRead;
  attempt: Attempt;
  comparison: Comparison;
  concept_request: ConceptRequest;
  confirm_account: ConfirmAccount;
  connect_account: ConnectAccount;
  created_token: CreatedToken;
  error: ErrorResponse;
  event: Event;
  final_result: FinalResult;
  issue_token: IssueToken;
  resume_request: ResumeRequest;
  run: Run;
  run_accepted: RunAccepted;
  service_status: ServiceStatus;
  service_stop_request: ServiceStopRequest;
  token_metadata: TokenMetadata;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "AccountReadiness".
 */
export interface AccountReadiness {
  active: number;
  capacity: number;
  cooldown_until_ms: number;
  id: string;
  next_submit_ms: number;
  rate_limit_count: number;
  readiness: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "AccountSession".
 */
export interface AccountSession {
  account_id: string;
  email: string;
  error_code?: string | null;
  lifecycle: string;
  login_expires_ms?: number | null;
  /**
   * Ephemeral supervisor observation; never persisted as account readiness.
   */
  login_view_available?: boolean;
  model?: ModelSelection | null;
  observation?: AccountObservation | null;
  observed_ms?: number | null;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "AccountObservation".
 */
export interface AccountObservation {
  available_models: string[];
  identity: AccountIdentity;
  model: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "AccountIdentity".
 */
export interface AccountIdentity {
  email: string;
  id: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ArtifactChunk".
 */
export interface ArtifactChunk {
  artifact: Artifact;
  eof: boolean;
  next_offset?: number | null;
  offset: number;
  text: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "Artifact".
 */
export interface Artifact {
  attempt_id?: string | null;
  byte_length: number;
  completion: string;
  created_ms: number;
  id: string;
  media_type: string;
  name: string;
  run_id: string;
  sha256: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ArtifactRead".
 */
export interface ArtifactRead {
  artifact_id: string;
  limit?: number;
  /**
   * UTF-8 byte offset, always use next_offset from the preceding response.
   */
  offset?: number;
  run_id: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "Attempt".
 */
export interface Attempt {
  accepted_ms?: number | null;
  completed_ms?: number | null;
  conversation_id?: string | null;
  conversation_url?: string | null;
  created_ms: number;
  error_code?: string | null;
  id: string;
  number: number;
  observed_model?: string | null;
  run_id: string;
  state: string;
  task_id: string;
  user_turn_id?: string | null;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "Comparison".
 */
export interface Comparison {
  basis: string;
  ranking: RankedCandidate[];
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "RankedCandidate".
 */
export interface RankedCandidate {
  evaluation: CandidateEvaluation;
  weighted_score: number;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "CandidateEvaluation".
 */
export interface CandidateEvaluation {
  candidate: number;
  disagreements: string[];
  scores: CriterionScore[];
  uncertainty: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "CriterionScore".
 */
export interface CriterionScore {
  criterion: string;
  rationale: string;
  score: number;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ConceptRequest".
 */
export interface ConceptRequest {
  account_id: string;
  candidate_count?: number;
  concept: string;
  constraints?: string;
  /**
   * @minItems 1
   * @maxItems 10
   */
  criteria?: Criterion[];
  idempotency_key: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "Criterion".
 */
export interface Criterion {
  name: string;
  weight: number;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ConfirmAccount".
 */
export interface ConfirmAccount {
  identity: AccountIdentity;
  model: ModelSelection;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ConnectAccount".
 */
export interface ConnectAccount {
  email: string;
  id?: string | null;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "CreatedToken".
 */
export interface CreatedToken {
  metadata: TokenMetadata;
  /**
   * Returned once. Only its SHA-256 digest is retained by the installation.
   */
  secret: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "TokenMetadata".
 */
export interface TokenMetadata {
  account_ids: string[];
  created_ms: number;
  id: string;
  name: string;
  revoked_ms?: number | null;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ErrorResponse".
 */
export interface ErrorResponse {
  code: string;
  message: string;
  next_action: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "Event".
 */
export interface Event {
  created_ms: number;
  data: unknown;
  kind: string;
  run_id: string;
  sequence: number;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "FinalResult".
 */
export interface FinalResult {
  artifacts: Artifact[];
  configuration: ConfigurationSnapshot;
  evaluation_notice: string;
  excluded_candidates: number[];
  /**
   * Accepted final text, populated when reading historical manifests too.
   */
  final_artifact?: Artifact | null;
  request: ConceptRequest;
  run_id: string;
  schema_version: number;
  status: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ConfigurationSnapshot".
 */
export interface ConfigurationSnapshot {
  max_attempts: number;
  max_jitter_ms: number;
  model: ModelSelection;
  overall_deadline_ms: number;
  perspectives: string[];
  response_timeout_ms: number;
  stage_char_limit: number;
  submission_spacing_ms: number;
  summary_char_limit: number;
  template_version: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "IssueToken".
 */
export interface IssueToken {
  /**
   * Explicit account IDs; future accounts are never added implicitly.
   */
  account_ids: string[];
  name: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ResumeRequest".
 */
export interface ResumeRequest {
  allow_incomplete?: boolean;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "Run".
 */
export interface Run {
  allow_incomplete: boolean;
  artifacts: Artifact[];
  configuration: ConfigurationSnapshot;
  created_ms: number;
  deadline_ms: number;
  id: string;
  pause_reason?: string | null;
  request: ConceptRequest;
  status: string;
  submission_limit: number;
  submissions: number;
  tasks: Task[];
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "Task".
 */
export interface Task {
  attempts: number;
  error_code?: string | null;
  id: string;
  position: number;
  stage: string;
  status: string;
  summary?: CandidateSummary | null;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "CandidateSummary".
 */
export interface CandidateSummary {
  assumptions: string[];
  next_steps: string[];
  proposal: string;
  strengths: string[];
  weaknesses: string[];
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "RunAccepted".
 */
export interface RunAccepted {
  result_url: string;
  run_id: string;
  run_url: string;
  status: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ServiceStatus".
 */
export interface ServiceStatus {
  instance_id?: string | null;
  pid?: number | null;
  runtime: string;
  state: ServiceState;
  version: string;
}
/**
 * This interface was referenced by `WorkflowContracts`'s JSON-Schema
 * via the `definition` "ServiceStopRequest".
 */
export interface ServiceStopRequest {
  instance_id: string;
}
