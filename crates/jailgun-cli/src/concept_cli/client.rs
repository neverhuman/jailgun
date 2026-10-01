use super::{error::failure, options::ConnectionOptions};
use anyhow::Result;
use reqwest::{Method, Url};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

pub struct Client {
    pub url: Url,
    http: reqwest::Client,
    token: String,
}
impl Client {
    pub async fn connect(options: &ConnectionOptions) -> Result<Self> {
        super::connect::connect(options).await
    }
    pub(crate) fn mcp_transport(
        &self,
    ) -> Result<rmcp::transport::StreamableHttpClientTransport<reqwest_mcp::Client>> {
        let mut config =
            rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
                self.url.join("/mcp")?.as_str(),
            )
            .auth_header(self.token.clone());
        config.allow_stateless = true;
        // MCP uses a different HTTP crate version. Its independent client must
        // preserve the same loopback-only credential boundary as the REST probe.
        let http = reqwest_mcp::Client::builder()
            .no_proxy()
            .redirect(reqwest_mcp::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .pool_max_idle_per_host(0)
            .build()?;
        Ok(rmcp::transport::StreamableHttpClientTransport::with_client(
            http, config,
        ))
    }
    pub(super) fn new(url: Url, token: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { url, http, token })
    }
    pub(super) async fn probe(&self) -> Result<bool> {
        // A bound listener may still be initializing storage. Transport failures
        // are read-only retries; only a refused connection permits auto-start.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
        loop {
            match self
                .http
                .get(self.url.join("/api/accounts")?)
                .bearer_auth(&self.token)
                .timeout(Duration::from_secs(3))
                .send()
                .await
            {
                Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => {
                    return Err(failure(
                        "unauthorized",
                        "The daemon rejected this credential.",
                        "Use the matching runtime or an unrevoked credential.",
                    ))
                }
                Ok(response) => {
                    let bytes = Self::read_response(response, 2 * 1024 * 1024).await?;
                    serde_json::from_slice::<Vec<jailgun_workflow::model::AccountReadiness>>(
                        &bytes,
                    )
                    .map_err(|_| {
                        failure(
                            "response-invalid",
                            "This service did not return concept account readiness.",
                            "Check the daemon version and selected address.",
                        )
                    })?;
                    return Ok(true);
                }
                Err(error) if error.is_connect() => return Ok(false),
                Err(_) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(150)).await
                }
                Err(_) => {
                    return Err(failure(
                        "daemon-unavailable",
                        "The daemon did not answer the authenticated probe.",
                        "Inspect the daemon and retry; no second daemon was started.",
                    ))
                }
            }
        }
    }
    pub async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> Result<T> {
        let bytes = self.bytes(method, path, body, 2 * 1024 * 1024).await?;
        serde_json::from_slice(&bytes).map_err(|_| {
            failure(
                "response-invalid",
                "The daemon returned an unexpected response.",
                "Check that client and daemon versions match.",
            )
        })
    }
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::GET, path, None::<&()>).await
    }
    pub async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        self.request(Method::POST, path, Some(body)).await
    }
    pub async fn artifact(&self, run: &str, id: &str) -> Result<Vec<u8>> {
        self.bytes(
            Method::GET,
            &format!("/api/runs/{run}/artifacts/{id}"),
            None::<&()>,
            32 * 1024 * 1024,
        )
        .await
    }
    async fn bytes(
        &self,
        method: Method,
        path: &str,
        body: Option<&impl Serialize>,
        limit: usize,
    ) -> Result<Vec<u8>> {
        let mut request = self
            .http
            .request(method, self.url.join(path)?)
            .bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|_| {
            failure(
                "daemon-unavailable",
                "The request did not return a response.",
                "Check the daemon. Retry submissions only with the identical idempotency key.",
            )
        })?;
        Self::read_response(response, limit).await
    }
    async fn read_response(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
        let status = response.status();
        let limit = if status.is_success() {
            limit
        } else {
            64 * 1024
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            failure(
                "response-interrupted",
                "The daemon response was interrupted.",
                "Reconnect and inspect the run before retrying.",
            )
        })? {
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err(failure(
                    "response-too-large",
                    "The response exceeds this client's read limit.",
                    "Inspect the registered result with the operator before exporting.",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
            return Err(failure(
                value["code"].as_str().unwrap_or("request-rejected"),
                value["message"]
                    .as_str()
                    .unwrap_or("The daemon rejected this operation."),
                value["next_action"]
                    .as_str()
                    .unwrap_or("Inspect the request and account readiness, then retry."),
            ));
        }
        Ok(bytes)
    }
}

pub fn loopback_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| {
        failure(
            "daemon-url-invalid",
            "The daemon URL is invalid.",
            "Use a loopback HTTP origin.",
        )
    })?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(failure(
            "daemon-url-invalid",
            "Credentials may only be sent to a loopback HTTP origin.",
            "Use an SSH local forward for remote access.",
        ));
    }
    Ok(url)
}
