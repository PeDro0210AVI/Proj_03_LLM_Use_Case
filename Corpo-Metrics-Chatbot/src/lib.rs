//! Shared core of the chatbot: the Claude client, the Corpo-Metrics MCP
//! client and configuration. Used by the TUI (`main.rs`) and by the
//! evaluation harness (`src/bin/eval`).

pub mod claude;
pub mod config;
pub mod mcp;
pub mod pricing;
