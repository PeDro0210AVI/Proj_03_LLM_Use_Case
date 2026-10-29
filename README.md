# Corpo Metrics — LLM Use Case (CC3092 Proyecto 3)

A chat assistant that answers managers' questions about their company's
business metrics (sales, finance, customers, projects, teams, product
health, inventory) by having Claude call the tools of a small MCP server
over the company's data.

```
user ──► TUI chatbot ──► Claude (Messages API, cloud)
              │  ▲              │ tool_use
              ▼  │              ▼
        Corpo-Metrics MCP server (local, JSON-RPC over HTTP)
              │
              ▼
        data/dummy/*.json
```

## Repository layout

| Path                     | What it is                                                         |
| ------------------------ | ------------------------------------------------------------------ |
| `Corpo-Metrics/`         | MCP server (Rust): exposes 10 business-metric tools over HTTP      |
| `Corpo-Metrics-Chatbot/` | TUI chat client (Rust): Claude + the Corpo-Metrics tools           |
| `eval/`                  | Evaluation set, harness runner script and results                  |
| `docs/`                  | Assignment brief, written report draft, architecture diagram       |
| `flake.nix`              | Nix dev shell and packages for both crates                         |

Both crates are members of one Cargo workspace (`Cargo.toml`) and share
`Cargo.lock` and `target/`.

## Quick start

```sh
nix develop                                   # or use your own Rust toolchain (1.85+)

# terminal 1 — MCP server on 127.0.0.1:8080
cd Corpo-Metrics && cargo run

# terminal 2 — chatbot
cp Corpo-Metrics-Chatbot/.env.example Corpo-Metrics-Chatbot/.env   # set ANTHROPIC_API_KEY
cd Corpo-Metrics-Chatbot && cargo run
```

See each crate's README for details:
[server](Corpo-Metrics/README.md), [chatbot](Corpo-Metrics-Chatbot/README.md).

## Reproducing the evaluation

```sh
./eval/run_all.sh    # needs ANTHROPIC_API_KEY; spends API credits (≈ $4–6)
```

See [eval/README.md](eval/README.md) for the metrics and how each run is
scored, and [docs/REPORTE.md](docs/REPORTE.md) for the written report.

Never commit `.env` — it is git-ignored.
