import { useEffect, useId, useState } from 'react';
import type { Artifact, Attempt, Comparison, Run } from '../generated/workflow';
import { artifactPath, artifactText, explain, validate, WorkflowError } from './api';
import { Markdown } from './Markdown';

export function Results({ run, attempts, report, demoArtifacts }: { run: Run; attempts: Attempt[]; report: (error: unknown) => void; demoArtifacts?: Record<string, string> }) {
  const [selected, setSelected] = useState<string | null>(null);
  const final = run.artifacts.find(artifact => artifact.name === 'final.md' && artifact.completion === 'complete');
  const current = run.artifacts.find(artifact => artifact.id === selected) ?? final ?? run.artifacts[0];
  const excluded = run.tasks.filter(task => task.status === 'excluded');
  return <section aria-labelledby="results-title" className="conceptPanel">
    <p className="eyebrow">{run.request.account_id} · {run.status.replaceAll('-', ' ')}</p>
    <h2 id="results-title">{run.status === 'completed' ? 'Your final concept' : 'Retained results'}</h2>
    <p className="conceptExcerpt">{run.request.concept}</p>
    <p className="subtle">Scores and rankings are model-generated judgments. Output quality depends on the selected model, criteria and source evidence.</p>
    {excluded.length ? <p className="notice">This result uses an explicitly selected subset. Excluded candidates: {excluded.map(task => task.position).join(', ')}.</p> : null}
    {run.artifacts.length === 0 ? <div className="emptyState"><h3>No captured results yet</h3><p>Complete and partial responses appear here after they have been saved.</p></div> : <>
      <label htmlFor="artifact-select">Read an output</label><select id="artifact-select" value={current?.id} onChange={event => setSelected(event.target.value)}>
        {run.artifacts.map(artifact => <option key={artifact.id} value={artifact.id}>{artifact.name} · {artifact.completion}{attempts.some(a => a.id === artifact.attempt_id) ? ` · attempt ${attempts.find(a => a.id === artifact.attempt_id)!.number}` : ''}</option>)}
      </select>
      {current ? <ArtifactView key={current.id} runId={run.id} artifact={current} run={run} report={report} demoText={demoArtifacts?.[current.id]} /> : null}
      <details className="downloadList"><summary>Download all outputs individually</summary><ul>{run.artifacts.map(artifact => <li key={artifact.id}>
        {demoArtifacts ? <span>{artifact.name} · {artifact.completion}</span> : <a href={artifactPath(artifact)} download>{artifact.name} · {artifact.completion}</a>}
      </li>)}</ul><p className="subtle">For a verified directory export, use <code>jailgun runs export {run.id} --out results</code>.</p></details>
    </>}
  </section>;
}
function ArtifactView({ runId, artifact, run, report, demoText }: { runId: string; artifact: Artifact; run: Run; report: (error: unknown) => void; demoText?: string }) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let ignore = false; setText(null); setError(null);
    const load = demoText === undefined ? artifactText(runId, artifact) : Promise.resolve(demoText);
    void load.then(value => { if (!ignore) setText(value); }).catch(cause => {
      if (ignore) return;
      setError(cause); if (cause instanceof WorkflowError && cause.status === 401) report(cause);
    });
    return () => { ignore = true; };
  }, [runId, artifact.id, artifact.sha256, retry, demoText]);
  let comparison: Comparison | null = null;
  let invalidComparison = false;
  if (text !== null && artifact.name === 'comparison.json' && text.length <= 300000) {
    try { comparison = validate('comparison', JSON.parse(text)); } catch { invalidComparison = true; }
  }
  return <article className="artifactReader" aria-label={artifact.name}>
    <div className="cardHeading"><h3>{artifact.name}</h3><span className={`statusPill status-${artifact.completion}`}>{artifact.completion}</span></div>
    {artifact.completion === 'partial' ? <p className="notice">Partial response: the requested generation did not complete. This text is retained for inspection.</p> : null}
    {invalidComparison ? <p className="errorState" role="alert">This comparison does not match the result schema. The original text is shown below for inspection.</p> : null}
    {error ? <div role="alert" className="errorState"><p>{explain(error)}</p><button className="secondaryButton" onClick={() => setRetry(value => value + 1)}>Retry loading output</button></div>
      : text === null ? <p role="status">Loading and verifying output…</p>
        : text.length > 300000 ? <p>This output is too large to render in the dashboard. Download the complete file below.</p>
          : comparison ? <Ranking comparison={comparison} run={run} />
            : artifact.media_type.startsWith('text/markdown') ? <Markdown text={text} /> : <pre className="jsonOutput">{text}</pre>}
    {demoText === undefined ? <a className="downloadLink" href={artifactPath(artifact)} download>Download {artifact.name}</a> : null}
  </article>;
}
function Ranking({ comparison, run }: { comparison: Comparison; run: Run }) {
  const hintId = useId();
  return <div className="ranking"><p>{comparison.basis}</p><p className="subtle comparisonScrollHint" id={hintId}>Scroll across the table to read scores and rationales. With a keyboard, focus the table and use the arrow keys.</p><div className="tableScroll" role="region" aria-label="Candidate comparison" aria-describedby={hintId} tabIndex={0}><table><thead><tr><th scope="col">Rank</th><th scope="col">Candidate</th><th scope="col">Weighted score / 100</th><th scope="col">Rationale and uncertainty</th></tr></thead>
    <tbody>{comparison.ranking.map((row, index) => <tr key={row.evaluation.candidate}><td>{index + 1}</td><th scope="row">{row.evaluation.candidate}. {run.configuration.perspectives[row.evaluation.candidate - 1]}</th><td>{row.weighted_score.toFixed(2)}</td><td>
      <p>{row.evaluation.uncertainty}</p><details><summary>Criterion scores and disagreements</summary><ul>{row.evaluation.scores.map(score => <li key={score.criterion}><strong>{score.criterion}: {score.score}/100</strong> — {score.rationale}</li>)}</ul>
        <p>Disagreements: {row.evaluation.disagreements.length ? row.evaluation.disagreements.join('; ') : 'None reported by the model.'}</p></details>
    </td></tr>)}</tbody></table></div></div>;
}
