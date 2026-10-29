use anyhow::{Context, Result};

use crate::local_mcp::ServerSpec;

/// Runtime configuration, resolved from environment variables (optionally
/// loaded from a `.env` file in the working directory).
pub struct Config {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub mcp_server_name: String,
    pub mcp_server_url: String,
    pub mcp_auth_token: Option<String>,
    /// Directory exposed to the local filesystem MCP server. `None` disables it.
    pub filesystem_root: Option<String>,
    /// Git repository exposed to the local git MCP server. `None` disables it.
    pub git_repo_path: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        // Best-effort: pick up a local .env file if present. Missing file is fine.
        let _ = dotenvy::dotenv();

        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY is not set (export it, or put it in a .env file)")?;

        let model = std::env::var("CLAUDE_MODEL").unwrap_or_else(|_| "claude-opus-5".to_string());

        let max_tokens = std::env::var("CLAUDE_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4096);

        let mcp_server_name =
            std::env::var("MCP_SERVER_NAME").unwrap_or_else(|_| "pedro021-mcp".to_string());

        let mcp_server_url = std::env::var("MCP_SERVER_URL")
            .unwrap_or_else(|_| "https://mcp.pedro021.com".to_string());

        let mcp_auth_token = std::env::var("MCP_AUTH_TOKEN").ok();

        let filesystem_root = optional_path_var("MCP_FILESYSTEM_ROOT", ".");
        let git_repo_path = optional_path_var("MCP_GIT_REPO_PATH", ".");

        Ok(Self {
            api_key,
            model,
            max_tokens,
            mcp_server_name,
            mcp_server_url,
            mcp_auth_token,
            filesystem_root,
            git_repo_path,
        })
    }

    /// Local (stdio, child-process) MCP servers to spawn, per the resolved
    /// config. Empty when both are disabled.
    pub fn local_mcp_servers(&self) -> Vec<ServerSpec> {
        let mut specs = Vec::new();

        if let Some(root) = &self.filesystem_root {
            specs.push(ServerSpec {
                name: "filesystem".to_string(),
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-filesystem".to_string(),
                    root.clone(),
                ],
            });
        }

        if let Some(repo) = &self.git_repo_path {
            specs.push(ServerSpec {
                name: "git".to_string(),
                command: "uvx".to_string(),
                args: vec![
                    "mcp-server-git".to_string(),
                    "--repository".to_string(),
                    repo.clone(),
                ],
            });
        }

        specs
    }
}

/// Reads a path-valued env var, defaulting to `default` when unset and
/// treating an explicit empty string as "disabled" (`None`).
fn optional_path_var(key: &str, default: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if v.is_empty() => None,
        Ok(v) => Some(v),
        Err(_) => Some(default.to_string()),
    }
}
