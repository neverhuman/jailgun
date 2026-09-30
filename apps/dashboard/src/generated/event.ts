/** Generated from Rust by scripts/generate-contracts.mjs. DO NOT EDIT. */

/**
 * This interface was referenced by `JailgunEvent`'s JSON-Schema
 * via the `definition` "EventKind".
 */
export type EventKind =
  | "run-queued"
  | "run-started"
  | "browser-lease-acquired"
  | "browser-lease-released"
  | "tab-opened"
  | "prompt-submitted"
  | "tar-discovered"
  | "download-receipt"
  | "deploy-queued"
  | "remote-safety"
  | "deploy-finished"
  | "prompt-policy"
  | "rate-limit-detected"
  | "browser-log"
  | "auth-state"
  | "auth-action-needed"
  | "auth-code-requested"
  | "auth-code-submitted"
  | "auth-complete"
  | "auth-failed"
  | "session-expired"
  | "error";
/**
 * This interface was referenced by `JailgunEvent`'s JSON-Schema
 * via the `definition` "Severity".
 */
export type Severity = "debug" | "info" | "warn" | "error";

export interface JailgunEvent {
  fields: {
    [k: string]: string;
  };
  kind: EventKind;
  message: string;
  run_id: string;
  severity: Severity;
  tab_id?: number | null;
  timestamp: string;
  [k: string]: unknown;
}

export const EVENT_KINDS = ["run-queued","run-started","browser-lease-acquired","browser-lease-released","tab-opened","prompt-submitted","tar-discovered","download-receipt","deploy-queued","remote-safety","deploy-finished","prompt-policy","rate-limit-detected","browser-log","auth-state","auth-action-needed","auth-code-requested","auth-code-submitted","auth-complete","auth-failed","session-expired","error"] as const;
