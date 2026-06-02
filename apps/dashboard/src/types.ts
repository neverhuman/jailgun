export interface RunSnapshot {
  run_id: string;
  started_at: string;
  finished_at: string | null;
  status: string;
  batch_tabs: number;
  loop_count: number;
  planned_tabs: number;
  tabs: TabSnapshot[];
  deploy_queue: 'idle' | 'waiting' | 'running' | 'blocked' | 'done';
  denied_github_prompts: number;
  allowed_info_prompts: number;
  early_stops_succeeded: number;
  early_stops_attempted: number;
}

export interface TabSnapshot {
  tab_id: number;
  status: string;
  page_url: string;
  archive_sha256: string | null;
  download_latency_ms: number | null;
  deploy_status: string;
  prompt_policy_decision: string | null;
  early_stop_outcome: 'succeeded' | 'attempted' | null;
  browser_profile: string | null;
  browser_profile_dir: string | null;
  browser_slot: number | null;
  cdp_url: string | null;
}

export interface JailgunEvent {
  run_id: string;
  tab_id: number | null;
  timestamp: string;
  kind: string;
  severity: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  fields: Record<string, string>;
}

export interface ReceiptResponse {
  run_id: string;
  receipts: unknown[];
}

export interface RunHistoryEntry {
  run_id: string;
  started_at: string;
  finished_at: string | null;
  status: string;
  batch_tabs: number;
  loop_count: number;
  planned_tabs: number;
  total_tabs: number;
  tabs_passed: number;
  tabs_failed: number;
  tabs_pushed: number;
  deploy_queue_final: string;
  denied_github_prompts: number;
  allowed_info_prompts: number;
  early_stops_succeeded: number;
  early_stops_attempted: number;
  code_stats?: {
    total_files_changed: number;
    total_additions: number;
    total_deletions: number;
    total_test_count: number;
  } | null;
}
