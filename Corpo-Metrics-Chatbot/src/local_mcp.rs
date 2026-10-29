use std::collections::HashMap;

use anyhow::{Context, Result};
use rmcp::model::{CallToolRequestParams, ContentBlock};
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use rmcp::transport::TokioChildProcess;
use serde_json::{json, Value};
use tokio::process::Command;

pub struct ServerSpec {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

struct LocalServer {
    name: String,
    service: RunningService<RoleClient, ()>,
}

struct ToolLookup {
    server_index: usize,
    real_name: String,
}

pub struct LocalMcpRegistry {
    servers: Vec<LocalServer>,
    tools: HashMap<String, ToolLookup>,
    tool_defs: Vec<Value>,
}

impl LocalMcpRegistry {
    pub async fn connect(specs: Vec<ServerSpec>) -> Self {
        let mut servers = Vec::new();
        let mut tools = HashMap::new();
        let mut tool_defs = Vec::new();

        for spec in specs {
            match Self::connect_one(&spec).await {
                Ok((service, discovered)) => {
                    let server_index = servers.len();
                    let tool_count = discovered.len();
                    for tool in discovered {
                        let public_name = format!("{}__{}", spec.name, tool.name);
                        let schema = Value::Object((*tool.input_schema).clone());
                        tool_defs.push(json!({
                            "name": public_name,
                            "description": tool.description.as_deref().unwrap_or(""),
                            "input_schema": schema,
                        }));
                        tools.insert(
                            public_name,
                            ToolLookup {
                                server_index,
                                real_name: tool.name.to_string(),
                            },
                        );
                    }
                    println!(
                        "mcp: connected to local server '{}' ({tool_count} tools)",
                        spec.name
                    );
                    servers.push(LocalServer {
                        name: spec.name.clone(),
                        service,
                    });
                }
                Err(e) => {
                    eprintln!("warning: local MCP server '{}' unavailable: {e:#}", spec.name);
                }
            }
        }

        Self {
            servers,
            tools,
            tool_defs,
        }
    }

    async fn connect_one(
        spec: &ServerSpec,
    ) -> Result<(RunningService<RoleClient, ()>, Vec<rmcp::model::Tool>)> {
        let mut command = Command::new(&spec.command);
        command.args(&spec.args);

        let transport = TokioChildProcess::new(command).with_context(|| {
            format!("spawning `{} {}`", spec.command, spec.args.join(" "))
        })?;

        let service = ().serve(transport).await.with_context(|| {
            format!("initializing MCP handshake with '{}'", spec.name)
        })?;

        let tools = service
            .list_tools(None)
            .await
            .with_context(|| format!("listing tools from '{}'", spec.name))?
            .tools;

        Ok((service, tools))
    }

    /// Anthropic-shaped tool definitions for every discovered local tool,
    /// ready to merge into the request's `tools` array.
    pub fn tool_defs(&self) -> &[Value] {
        &self.tool_defs
    }

    /// Names of the local servers that connected successfully, for display.
    pub fn connected_names(&self) -> Vec<String> {
        self.servers.iter().map(|s| s.name.clone()).collect()
    }

    pub fn is_local_tool(&self, public_name: &str) -> bool {
        self.tools.contains_key(public_name)
    }

    /// Executes a tool by its public (`server__tool`) name. Never fails
    /// outward — a connection problem or unknown tool becomes an
    /// `is_error: true` result so Claude can see and react to it, the same
    /// as any other tool failure.
    pub async fn call(&self, public_name: &str, arguments: Value) -> (String, bool) {
        let Some(lookup) = self.tools.get(public_name) else {
            return (format!("unknown local tool '{public_name}'"), true);
        };
        let Some(server) = self.servers.get(lookup.server_index) else {
            return (
                format!("server for tool '{public_name}' is no longer available"),
                true,
            );
        };

        let mut params = CallToolRequestParams::new(lookup.real_name.clone());
        if let Some(map) = arguments.as_object().cloned() {
            params = params.with_arguments(map);
        }

        match server.service.call_tool(params).await {
            Ok(result) => {
                let text = render_content(&result.content);
                (text, result.is_error.unwrap_or(false))
            }
            Err(e) => (format!("tool call failed: {e}"), true),
        }
    }
}

fn render_content(blocks: &[ContentBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if !out.is_empty() {
            out.push('\n');
        }
        match block {
            ContentBlock::Text(t) => out.push_str(&t.text),
            ContentBlock::Image(_) => out.push_str("[image content omitted]"),
            ContentBlock::Audio(_) => out.push_str("[audio content omitted]"),
            ContentBlock::Resource(_) => out.push_str("[embedded resource omitted]"),
            ContentBlock::ResourceLink(_) => out.push_str("[resource link omitted]"),
            _ => out.push_str("[unsupported content omitted]"),
        }
    }
    out
}

#[cfg(test)]
mod smoke_test {
    use super::*;

    /// Spawns the real filesystem/git MCP servers and checks the handshake
    /// and tool discovery actually work end-to-end. Not run by default,
    /// since it needs `npx`/`uvx` (both provided by `nix develop`) and
    /// network access to fetch them the first time:
    ///
    /// ```sh
    /// cargo test --bin corpo-metrics-chatbot -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "spawns real npx/uvx processes; requires network on first run"]
    async fn connects_and_lists_tools() {
        let specs = vec![
            ServerSpec {
                name: "filesystem".to_string(),
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                    ".".to_string(),
                ],
            },
            ServerSpec {
                name: "git".to_string(),
                command: "uvx".to_string(),
                args: vec![
                    "mcp-server-git".to_string(),
                    "--repository".to_string(),
                    ".".to_string(),
                ],
            },
        ];
        let registry = LocalMcpRegistry::connect(specs).await;
        eprintln!("connected: {:?}", registry.connected_names());
        eprintln!("tool defs: {}", registry.tool_defs().len());
        for def in registry.tool_defs() {
            eprintln!(" - {}", def["name"]);
        }
        assert_eq!(registry.connected_names().len(), 2, "both servers should connect");
        assert!(!registry.tool_defs().is_empty());
    }
}
