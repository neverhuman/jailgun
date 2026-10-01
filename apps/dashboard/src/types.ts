export interface RunSnapshot {
  run_id: string;
  started_at: string;
  finished_at: string | null;
  status: string;
  tabs: TabSnapshot[];
  deploy_queue: 'idle' | 'waiting' | 'running' | 'blocked' | 'done';
  denied_github_prompts: number;
  allowed_info_prompts: number;
}

export interface TabSnapshot {
  tab_id: number;
  status: string;
  page_url: string;
  archive_sha256: string | null;
  download_latency_ms: number | null;
  deploy_status: string;
  prompt_policy_decision: string | null;
}

export type { JailgunEvent } from './generated/event';

export interface ReceiptResponse {
  run_id: string;
  receipts: unknown[];
}

