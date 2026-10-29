# Corpo Metrics Chatbot

A terminal UI (TUI) chat client, written in Rust, that talks to Claude
through the Anthropic Messages API and answers questions about business
metrics using the tools of the **Corpo-Metrics MCP server**
(`../Corpo-Metrics`), which runs locally on your machine.

Anthropic's servers can't reach `localhost`, so the app itself is the MCP
client: it performs the MCP handshake with the local server, hands its
tools to Claude as ordinary client-side tools, and executes every tool call
Claude makes. Corpo-Metrics is the only MCP server the chatbot uses.

## Requirements

- [Nix](https://nixos.org/) with flakes enabled (`nix develop` at the repo
  root), **or** a local Rust toolchain (1.85+).
- An Anthropic API key ([console.anthropic.com](https://console.anthropic.com)).
- The Corpo-Metrics MCP server running (see below).

## Setup

1. Copy the example environment file and fill in your API key:

   ```sh
   cp .env.example .env
   $EDITOR .env
   ```

   Only `ANTHROPIC_API_KEY` is required. `MCP_SERVER_URL` defaults to
   `http://localhost:8080`.

2. Start the MCP server in another terminal (it reads `data/` relative to
   its working directory, so run it from its own folder):

   ```sh
   cd ../Corpo-Metrics && cargo run
   ```

## Running

```sh
cargo run -p corpo-metrics-chatbot
# or build once and run the packaged binary:
nix build .#corpo-metrics-chatbot
./result/bin/corpo-metrics-chatbot
```

Environment variables are read from the process environment first and
fall back to a `.env` file in the current directory. If the MCP server is
not reachable, the app exits before opening the TUI with a message saying
how to start it.

## Usage

- Type your message and press **Enter** to send it.
- While Claude is replying, the input box is locked; incoming text streams
  in live, and a status line shows when Claude is calling an MCP tool.
- After each reply a line shows that turn's latency, input/output tokens,
  number of API calls and estimated cost.
- **↑ / ↓** scroll the conversation one line at a time, **PageUp / PageDown**
  scroll by a page, **End** jumps back to the latest message.
- **Esc** or **Ctrl-C** quits.

## Configuration reference

All configuration is via environment variables (see `.env.example`):

| Variable            | Required | Default                 | Description                                         |
| ------------------- | -------- | ----------------------- | --------------------------------------------------- |
| `ANTHROPIC_API_KEY` | yes      | —                       | Your Anthropic API key                              |
| `CLAUDE_MODEL`      | no       | `claude-opus-5`         | Model ID to send requests to                        |
| `CLAUDE_MAX_TOKENS` | no       | `4096`                  | Max output tokens per reply                         |
| `MCP_SERVER_URL`    | no       | `http://localhost:8080` | URL of the local Corpo-Metrics MCP server           |
| `MCP_AUTH_TOKEN`    | no       | —                       | Bearer token for the MCP server, if it requires one |

## Project layout

```
prompts/
└── system_v2.md   current system prompt (versioned; see docs/REPORTE.md)
src/
├── lib.rs         shared core used by the TUI and the evaluation harness
├── main.rs        TUI entry point, MCP connection, terminal setup, event loop
├── app.rs         application state (conversation history, input, scroll)
├── ui.rs          ratatui rendering
├── claude.rs      Claude API client (SSE streaming, usage accounting and
│                  the client-side tool-use loop that executes MCP calls)
├── mcp.rs         MCP client for Corpo-Metrics (JSON-RPC over HTTP:
│                  initialize, tools/list, tools/call)
├── pricing.rs     token usage and per-model cost estimate
├── config.rs      environment-variable configuration
└── bin/eval/      evaluation harness (see ../eval/README.md)
```

## How it works

1. On startup `McpClient::connect` sends `initialize`,
   `notifications/initialized` and `tools/list` to the MCP server and turns
   each tool into a Messages API tool definition
   (`name`, `description`, `input_schema`).
2. Every `POST /v1/messages` request carries a system prompt (answer only
   from tool data, reply in the user's language) and those tools:

   ```json
   {
     "model": "claude-opus-5",
     "system": "You are Corpo Metrics, ...",
     "tools": [
       { "name": "get_sales_metrics", "description": "...", "input_schema": { "...": "..." } }
     ],
     "stream": true
   }
   ```

3. When Claude stops with `stop_reason: "tool_use"`,
   `ClaudeClient::run_turn` executes each `tool_use` block as an MCP
   `tools/call`, returns all results in one `tool_result` message, and
   continues — repeating until the model is done or a safety cap of 10
   tool round trips is hit.

## Tests

With the MCP server running:

```sh
cargo test -p corpo-metrics-chatbot -- --ignored --nocapture
```

runs a smoke test that connects, lists the tools and makes a real call.

## License

MIT
