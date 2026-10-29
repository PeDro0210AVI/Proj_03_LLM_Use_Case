use serde_json::{json, Value};

use crate::claude::ApiEvent;
use crate::config::Config;
use crate::local_mcp::LocalMcpRegistry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub role: Role,
    pub text: String,
}

pub struct App {
    pub header: String,
    pub history: Vec<Value>,
    pub display: Vec<DisplayMessage>,
    pub input: String,
    pub is_streaming: bool,
    pub status: Option<String>,
    pub scroll_offset: usize,
    pub max_scroll: usize,
    pub follow: bool,
    pub should_quit: bool,

    open_text_idx: Option<usize>,
}

impl App {
    pub fn new(config: &Config, local: &LocalMcpRegistry) -> Self {
        let local_names = local.connected_names();
        let header = if local_names.is_empty() {
            format!(
                "corpo-metrics-chatbot — model: {} — mcp: {}",
                config.model, config.mcp_server_url
            )
        } else {
            format!(
                "corpo-metrics-chatbot — model: {} — mcp: {} + local[{}]",
                config.model,
                config.mcp_server_url,
                local_names.join(", ")
            )
        };

        Self {
            header,
            history: Vec::new(),
            display: vec![DisplayMessage {
                role: Role::System,
                text: "Connected. Type a message and press Enter to send. \
                       Ctrl-C or Esc to quit."
                    .to_string(),
            }],
            input: String::new(),
            is_streaming: false,
            status: None,
            scroll_offset: 0,
            max_scroll: 0,
            follow: true,
            should_quit: false,
            open_text_idx: None,
        }
    }

    /// Takes the current input box contents as a new user turn, appends it
    /// to both the display log and API history, and returns a clone of the
    /// full history ready to send.
    pub fn submit_input(&mut self) -> Option<Vec<Value>> {
        if self.is_streaming {
            return None;
        }
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.input.clear();

        self.display.push(DisplayMessage {
            role: Role::User,
            text: text.clone(),
        });
        self.history.push(json!({"role": "user", "content": text}));
        self.is_streaming = true;
        self.open_text_idx = None;
        self.follow = true;
        self.scroll_offset = 0;

        Some(self.history.clone())
    }

    pub fn handle_api_event(&mut self, event: ApiEvent) {
        match event {
            ApiEvent::TextDelta(text) => {
                let idx = match self.open_text_idx {
                    Some(idx) => idx,
                    None => {
                        self.display.push(DisplayMessage {
                            role: Role::Assistant,
                            text: String::new(),
                        });
                        let idx = self.display.len() - 1;
                        self.open_text_idx = Some(idx);
                        idx
                    }
                };
                self.display[idx].text.push_str(&text);
            }
            ApiEvent::ToolUse { name } => {
                self.open_text_idx = None;
                self.status = Some(format!("calling MCP tool: {name}"));
                self.display.push(DisplayMessage {
                    role: Role::Tool,
                    text: format!("→ calling MCP tool: {name}"),
                });
            }
            ApiEvent::ToolResult { is_error } => {
                self.status = None;
                self.display.push(DisplayMessage {
                    role: Role::Tool,
                    text: if is_error {
                        "✗ MCP tool call failed".to_string()
                    } else {
                        "✓ MCP tool result received".to_string()
                    },
                });
            }
            ApiEvent::Done { new_messages } => {
                // May be more than one assistant/user pair if the turn
                // involved client-side (local MCP) tool round trips.
                self.history.extend(new_messages);
                self.is_streaming = false;
                self.open_text_idx = None;
                self.status = None;
            }
            ApiEvent::Error(msg) => {
                self.display.push(DisplayMessage {
                    role: Role::System,
                    text: format!("Error: {msg}"),
                });
                self.is_streaming = false;
                self.open_text_idx = None;
                self.status = None;
            }
        }
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.follow = false;
        self.scroll_offset = (self.scroll_offset + lines).min(self.max_scroll);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        if self.scroll_offset == 0 {
            self.follow = true;
        }
    }

    pub fn jump_to_bottom(&mut self) {
        self.follow = true;
        self.scroll_offset = 0;
    }
}
