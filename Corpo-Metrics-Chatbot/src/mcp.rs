use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Minimal MCP client for the Corpo-Metrics server, which speaks plain
/// JSON-RPC over HTTP POST (one request → one `application/json` reply, no
/// SSE, no sessions). This is the only MCP server the chatbot talks to.
pub struct McpClient {
    http: reqwest::Client,
    url: String,
    auth_token: Option<String>,
    next_id: AtomicU32,
    tool_names: HashSet<String>,
    tool_defs: Vec<Value>,
}

impl McpClient {
    /// Runs the MCP handshake and discovers the server's tools.
    pub async fn connect(url: &str, auth_token: Option<String>) -> Result<Self> {
        // The server closes the TCP connection after every response, so a
        // pooled keep-alive connection would just be a stale socket.
        let http = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .build()
            .context("building HTTP client")?;

        let mut client = Self {
            http,
            url: url.to_string(),
            auth_token,
            next_id: AtomicU32::new(1),
            tool_names: HashSet::new(),
            tool_defs: Vec::new(),
        };

        client
            .request(
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {
                        "name": env!("CARGO_PKG_NAME"),
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await
            .context("MCP initialize")?;
        client
            .notify("notifications/initialized")
            .await
            .context("MCP initialized notification")?;

        let listed = client
            .request("tools/list", json!({}))
            .await
            .context("MCP tools/list")?;
        let tools = listed
            .get("tools")
            .and_then(Value::as_array)
            .context("tools/list result has no `tools` array")?;

        for tool in tools {
            let Some(name) = tool.get("name").and_then(Value::as_str) else {
                continue;
            };
            let schema = tool
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
            client.tool_defs.push(json!({
                "name": name,
                "description": tool.get("description").and_then(Value::as_str).unwrap_or(""),
                "input_schema": schema,
            }));
            client.tool_names.insert(name.to_string());
        }

        Ok(client)
    }

    /// Tool definitions in the shape the Messages API `tools` array expects.
    pub fn tool_defs(&self) -> &[Value] {
        &self.tool_defs
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Executes a tool by name. Never fails outward — an unknown tool or a
    /// transport/JSON-RPC error becomes an `is_error: true` result so the
    /// model can see it and react, the same as any other tool failure.
    pub async fn call(&self, name: &str, arguments: Value) -> (String, bool) {
        if !self.tool_names.contains(name) {
            return (format!("unknown tool '{name}'"), true);
        }

        let arguments = if arguments.is_object() { arguments } else { json!({}) };
        match self
            .request("tools/call", json!({"name": name, "arguments": arguments}))
            .await
        {
            Ok(result) => {
                let is_error = result.get("isError").and_then(Value::as_bool).unwrap_or(false);
                (render_content(&result), is_error)
            }
            Err(e) => (format!("tool call failed: {e:#}"), true),
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});

        let response: Value = self
            .post(&body)
            .send()
            .await
            .with_context(|| format!("POST {}", self.url))?
            .json()
            .await
            .context("decoding JSON-RPC response")?;

        if let Some(err) = response.get("error") {
            bail!(
                "{} (code {})",
                err.get("message").and_then(Value::as_str).unwrap_or("unknown error"),
                err.get("code").unwrap_or(&Value::Null)
            );
        }
        response
            .get("result")
            .cloned()
            .context("JSON-RPC response has neither `result` nor `error`")
    }

    async fn notify(&self, method: &str) -> Result<()> {
        self.post(&json!({"jsonrpc": "2.0", "method": method}))
            .send()
            .await
            .with_context(|| format!("POST {}", self.url))?
            .error_for_status()?;
        Ok(())
    }

    fn post(&self, body: &Value) -> reqwest::RequestBuilder {
        let req = self.http.post(&self.url).json(body);
        match &self.auth_token {
            Some(token) => req.bearer_auth(token),
            None => req,
        }
    }
}

/// Flattens an MCP `tools/call` result's content blocks into plain text.
fn render_content(result: &Value) -> String {
    let Some(blocks) = result.get("content").and_then(Value::as_array) else {
        return result.to_string();
    };
    blocks
        .iter()
        .map(|block| match block.get("text").and_then(Value::as_str) {
            Some(text) => text.to_string(),
            None => "[non-text content omitted]".to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod smoke_test {
    use super::*;

    /// Talks to a running Corpo-Metrics server and checks the handshake,
    /// tool discovery and a real tool call work end-to-end. Not run by
    /// default, since it needs the server up:
    ///
    /// ```sh
    /// (cd Corpo-Metrics && cargo run) &
    /// cargo test -p corpo-metrics-chatbot -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs the Corpo-Metrics server running on localhost:8080"]
    async fn connects_lists_and_calls_tools() {
        let client = McpClient::connect("http://localhost:8080", None)
            .await
            .expect("connect");
        for def in client.tool_defs() {
            eprintln!(" - {}", def["name"]);
        }
        assert!(!client.tool_defs().is_empty());

        let (text, is_error) = client
            .call("get_sales_metrics", json!({"period": "month"}))
            .await;
        eprintln!("get_sales_metrics → {text}");
        assert!(!is_error, "tool call failed: {text}");
        assert!(!text.is_empty());
    }
}
