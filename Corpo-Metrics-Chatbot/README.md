# Corpo Metrics Chatbot

A terminal UI (TUI) chat client, written in Rust, that talks to Claude
through the Anthropic Messages API and gives it access to three MCP (Model
Context Protocol) servers:

- **`mcp.pedro021.com`** — a remote HTTP server, connected via Anthropic's
  server-side [MCP connector](https://platform.claude.com/docs/en/agents-and-tools/tool-use/model-context-protocol).
  Anthropic talks to it directly; this client just tells the API where it is.
- **Filesystem** ([`@modelcontextprotocol/server-filesystem`](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem)) —
  read/write access to a directory on this machine.
- **Git** ([`mcp-server-git`](https://github.com/modelcontextprotocol/servers-archived/tree/main/src/git)) —
  status/diff/commit/log/etc. on a local git repository.

The filesystem and git servers are *local* (they run as child processes on
this machine and speak MCP over stdio), so the app itself acts as their MCP
client — it drives the tool-call loop, unlike the remote server where
Anthropic does that server-side.

## Requirements

- [Nix](https://nixos.org/) with flakes enabled (the project is built and
  run through the provided `flake.nix`), **or** a local Rust toolchain
  (1.75+) if you'd rather not use Nix.
- An Anthropic API key ([console.anthropic.com](https://console.anthropic.com)).
- Network access to `mcp.pedro021.com`.
- `npx` (Node.js) and `uvx` ([uv](https://docs.astral.sh/uv/)) on `PATH`, to
  spawn the filesystem and git MCP servers. Both are provided automatically
  inside `nix develop`, and are bundled into the `nix build` output too.

## Setup

1. Copy the example environment file and fill in your API key:

   ```sh
   cp .env.example .env
   $EDITOR .env
   ```

   At minimum you need `ANTHROPIC_API_KEY` set. `MCP_SERVER_URL` already
   defaults to `https://mcp.pedro021.com` — only change it if the server
   exposes a different path, or set `MCP_AUTH_TOKEN` if it requires a
   bearer token. `MCP_FILESYSTEM_ROOT` and `MCP_GIT_REPO_PATH` default to
   `.` (the current directory); set either to an empty string to disable
   that server.

2. Enter the dev shell and build:

   ```sh
   nix develop
   cargo build
   ```

## Running

```sh
nix develop -c cargo run
# or, from inside `nix develop`:
cargo run

# or build once and run the packaged binary:
nix build
./result/bin/corpo-metrics-chatbot
```

Environment variables are read from the process environment first and
fall back to a `.env` file in the current directory. On startup, before the
TUI takes over the terminal, the app prints which local MCP servers
connected (and a warning for any that failed to start — e.g. `npx`/`uvx`
missing, or the target directory/repo not existing) so it's obvious what
went wrong if a server doesn't come up.

## Usage

- Type your message and press **Enter** to send it.
- While Claude is replying, the input box is locked; incoming text streams
  in live, and a status line shows when Claude is calling a tool — on the
  remote MCP server, or on the local filesystem/git ones.
- **↑ / ↓** scroll the conversation one line at a time, **PageUp / PageDown**
  scroll by a page, **End** jumps back to the latest message.
- **Esc** or **Ctrl-C** quits.

## Configuration reference

All configuration is via environment variables (see `.env.example`):

| Variable              | Required | Default                      | Description                                        |
| ---------------------- | -------- | ----------------------------- | --------------------------------------------------- |
| `ANTHROPIC_API_KEY`     | yes      | —                              | Your Anthropic API key                               |
| `CLAUDE_MODEL`          | no       | `claude-opus-5`                | Model ID to send requests to                         |
| `CLAUDE_MAX_TOKENS`     | no       | `4096`                         | Max output tokens per reply                          |
| `MCP_SERVER_URL`        | no       | `https://mcp.pedro021.com`     | URL of the remote MCP server                         |
| `MCP_SERVER_NAME`       | no       | `pedro021-mcp`                 | Internal name Claude uses to refer to the server     |
| `MCP_AUTH_TOKEN`        | no       | —                              | Bearer token for the remote MCP server, if required  |
| `MCP_FILESYSTEM_ROOT`   | no       | `.`                            | Directory exposed to the filesystem MCP server. Empty string disables it. |
| `MCP_GIT_REPO_PATH`     | no       | `.`                            | Git repository exposed to the git MCP server. Empty string disables it. |

## Project layout

```
src/
├── main.rs      entry point, terminal setup, event loop
├── app.rs       application state (conversation history, input, scroll)
├── ui.rs        ratatui rendering
├── claude.rs    Claude API client (SSE streaming, MCP connector wiring,
│                the client-side tool-use loop for local MCP tools)
├── local_mcp.rs local MCP client: spawns/talks to the filesystem and git
│                servers over stdio (via the `rmcp` crate) and exposes
│                their tools to Claude
└── config.rs    environment-variable configuration
```

## How it works

Every request to `POST /v1/messages` declares the remote server via the MCP
connector, plus (if connected) the local servers' tools as ordinary
client-side tools:

```json
{
  "model": "claude-opus-5",
  "mcp_servers": [
    { "type": "url", "name": "pedro021-mcp", "url": "https://mcp.pedro021.com" }
  ],
  "tools": [
    { "type": "mcp_toolset", "mcp_server_name": "pedro021-mcp" },
    { "name": "filesystem__read_text_file", "description": "...", "input_schema": { "...": "..." } },
    { "name": "git__git_status", "description": "...", "input_schema": { "...": "..." } }
  ],
  "stream": true
}
```

with the beta header `anthropic-beta: mcp-client-2025-11-20`.

- **Remote tool calls** (`mcp_tool_use` / `mcp_tool_result` blocks) are
  resolved entirely by Anthropic — the client just streams the text and
  status back.
- **Local tool calls** end the turn with `stop_reason: "tool_use"`. The
  client (`ClaudeClient::run_turn`) executes the call against the matching
  local MCP server, sends the result back as a `tool_result`, and continues
  the conversation — repeating until the model is done or a safety cap on
  tool round trips is hit.

Local tool names are prefixed by server (`filesystem__...`, `git__...`) to
keep them unique and to route each call to the right child process.

## License

MIT
