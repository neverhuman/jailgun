import validators from '../generated/workflow-validators.cjs';
import type { Artifact, WorkflowContracts } from '../generated/workflow';

export class WorkflowError extends Error {
  constructor(public code: string, message: string, public nextAction: string, public status = 0) { super(message); }
}
export function explain(error: unknown): string {
  return error instanceof WorkflowError ? `${error.message} ${error.nextAction}` : 'The operation could not be completed. Reconnect and try again.';
}
export function validate<K extends keyof WorkflowContracts>(kind: K, value: unknown): WorkflowContracts[K] {
  if (!validators[kind](value)) throw new WorkflowError('response-invalid', 'The service returned an invalid response.', 'Check that the dashboard and daemon versions match.');
  return value as WorkflowContracts[K];
}
async function response(path: string, method: string, body?: unknown): Promise<Response> {
  let result: Response;
  try {
    result = await fetch(path, { method, credentials: 'same-origin', cache: 'no-store', redirect: 'error', signal: AbortSignal.timeout(30_000),
      ...(body === undefined ? {} : { headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }) });
  } catch {
    throw new WorkflowError('connection-unavailable', 'The local service could not be reached.', 'Check that Jailgun is running, then retry.');
  }
  if (!result.ok) {
    const parsed: unknown = await boundedBytes(result, 64 * 1024).then(bytes => JSON.parse(new TextDecoder().decode(bytes))).catch(() => ({}));
    const value = parsed && typeof parsed === 'object' ? parsed as Record<string, unknown> : {};
    throw new WorkflowError(typeof value.code === 'string' ? value.code : 'request-rejected', typeof value.message === 'string' ? value.message : `The service rejected this operation (${result.status}).`, typeof value.next_action === 'string' ? value.next_action : 'Check account readiness and try again.', result.status);
  }
  return result;
}
async function json(path: string, method: string, body?: unknown): Promise<unknown> {
  const result = await response(path, method, body);
  const text = await boundedBytes(result, 2 * 1024 * 1024);
  try { return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(text)); }
  catch { throw new WorkflowError('response-invalid', 'The service returned invalid JSON.', 'Check the daemon and reconnect.'); }
}
export async function request<K extends keyof WorkflowContracts>(kind: K, path: string, method = 'GET', body?: unknown): Promise<WorkflowContracts[K]> {
  return validate(kind, await json(path, method, body));
}
export async function list<K extends keyof WorkflowContracts>(kind: K, path: string): Promise<WorkflowContracts[K][]> {
  const value = await json(path, 'GET');
  if (!Array.isArray(value)) throw new WorkflowError('response-invalid', 'The service returned an invalid list.', 'Check the daemon and reconnect.');
  return value.map(item => validate(kind, item));
}
export async function mutate(path: string, body: unknown = {}, method = 'POST'): Promise<void> { await response(path, method, body); }
export function runPath(id: string): string { return `/api/runs/${encodeURIComponent(id)}`; }
export function artifactPath(artifact: Artifact): string { return `${runPath(artifact.run_id)}/artifacts/${encodeURIComponent(artifact.id)}`; }
export async function artifactText(runId: string, artifact: Artifact): Promise<string> {
  if (runId !== artifact.run_id) throw new WorkflowError('artifact-ownership', 'This artifact belongs to another run.', 'Reload the selected run.');
  const bytes = await boundedBytes(await response(artifactPath(artifact), 'GET'), 32 * 1024 * 1024);
  const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(v => v.toString(16).padStart(2, '0')).join('');
  if (bytes.byteLength !== artifact.byte_length || hash !== artifact.sha256) throw new WorkflowError('artifact-integrity', 'This result failed its integrity check.', 'Retain the server data and inspect a verified backup.');
  try { return new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
  catch { throw new WorkflowError('artifact-invalid-text', 'The result is not valid UTF-8.', 'Inspect the stored artifact.'); }
}
async function boundedBytes(result: Response, maximum: number): Promise<Uint8Array<ArrayBuffer>> {
  const reader = result.body?.getReader();
  if (!reader) throw new WorkflowError('response-empty', 'The service returned an empty response.', 'Reconnect and inspect the run.');
  const chunks: Uint8Array[] = []; let length = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read(); if (done) break;
      length += value.length;
      if (length > maximum) throw new WorkflowError('response-too-large', 'This response is too large to display.', 'Use the command line to inspect or export the result.');
      chunks.push(value);
    }
  } catch (error) {
    await reader.cancel().catch(() => {});
    if (error instanceof WorkflowError) throw error;
    throw new WorkflowError('response-interrupted', 'The service response was interrupted.', 'Reconnect and inspect the run before retrying.');
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(length); let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return bytes;
}
