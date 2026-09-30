use super::*;

pub(super) async fn review_patch(router_url: &str, diff: &str) -> Result<ReviewGateResult> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": "jailhard-review",
        "method": "tools/call",
        "params": {
            "name": "review_patch",
            "arguments": {
                "diff": diff,
                "max_workers": 3,
                "focus": [
                    "correctness",
                    "regression risk",
                    "security",
                    "tests",
                    "performance",
                    "Jankurai standard"
                ]
            }
        }
    });
    let response: Value = reqwest::Client::new()
        .post(router_url)
        .json(&request)
        .send()
        .await
        .with_context(|| format!("calling router review gate at {router_url}"))?
        .error_for_status()
        .with_context(|| format!("router review gate returned HTTP error at {router_url}"))?
        .json()
        .await
        .context("decoding router review gate response")?;
    if let Some(error) = response.get("error") {
        anyhow::bail!("router review gate returned JSON-RPC error: {error}");
    }
    let result = response
        .get("result")
        .context("router response missing result")?;
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or_default();
    if is_error {
        anyhow::bail!("router review gate returned isError=true: {result}");
    }
    let structured = match result.get("structuredContent") {
        Some(structured) => structured,
        None => result
            .get("structured_content")
            .context("router response missing structuredContent")?,
    };
    let status = structured
        .get("status")
        .and_then(Value::as_str)
        .context("router response missing structuredContent.status")?
        .to_string();
    if status != "succeeded" {
        anyhow::bail!("router review gate status was {status:?}, expected succeeded");
    }
    let worker_count = worker_count(result, structured);
    if worker_count < 3 {
        anyhow::bail!("router review gate used {worker_count} worker(s), expected at least 3");
    }
    let high_risk_supporting_models = high_risk_supporting_models(structured);
    Ok(ReviewGateResult {
        status,
        worker_count,
        router_job_id: router_job_id(structured),
        high_risk_supporting_models,
    })
}

pub(super) fn worker_count(result: &Value, structured: &Value) -> u64 {
    if let Some(worker_count) = structured
        .pointer("/telemetry/worker_count")
        .and_then(Value::as_u64)
    {
        return worker_count;
    }
    result
        .pointer("/telemetry/worker_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

pub(super) fn router_job_id(structured: &Value) -> Option<String> {
    if let Some(job_id) = structured.get("job_id").and_then(Value::as_str) {
        return Some(job_id.to_string());
    }
    structured
        .pointer("/job/id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn high_risk_supporting_models(structured: &Value) -> u64 {
    let Some(findings) = review_findings(structured) else {
        return 0;
    };
    findings
        .iter()
        .filter(|finding| high_risk_finding(finding))
        .map(supporting_model_count)
        .sum()
}

fn review_findings(structured: &Value) -> Option<&Vec<Value>> {
    if let Some(findings) = structured.get("findings").and_then(Value::as_array) {
        return Some(findings);
    }
    if let Some(findings) = structured.get("issues").and_then(Value::as_array) {
        return Some(findings);
    }
    structured
        .pointer("/verdict/findings")
        .and_then(Value::as_array)
}

pub(super) fn high_risk_finding(finding: &Value) -> bool {
    let severity = match finding.get("severity").and_then(Value::as_str) {
        Some(severity) => severity.to_ascii_lowercase(),
        None => String::new(),
    };
    if severity != "high" && severity != "critical" {
        return false;
    }
    let category = [
        finding.get("category"),
        finding.get("focus"),
        finding.get("type"),
        finding.get("risk"),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .collect::<Vec<_>>()
    .join(" ")
    .to_ascii_lowercase();
    category.contains("correctness")
        || category.contains("regression")
        || category.contains("security")
}

pub(super) fn supporting_model_count(finding: &Value) -> u64 {
    if let Some(models) = finding.get("supporting_models").and_then(Value::as_array) {
        return models.len() as u64;
    }
    if let Some(models) = finding.get("models").and_then(Value::as_array) {
        return models.len() as u64;
    }
    if let Some(count) = finding.get("support_count").and_then(Value::as_u64) {
        return count;
    }
    finding
        .get("supporting_model_count")
        .and_then(Value::as_u64)
        .unwrap_or(1)
}
