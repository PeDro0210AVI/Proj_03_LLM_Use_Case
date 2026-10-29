MCP server that exposes corporate business metrics to any MCP-compatible AI client.

## Running

```bash
cargo run
```

Starts an HTTP server on `127.0.0.1:8080`. The data is read from `data/dummy/` JSON files.

## Connecting as MCP server

Add to your client's MCP config (`.mcp.json` is already set up for Claude Code):

```json
{
  "mcpServers": {
    "corpo-metrics": {
      "type": "http",
      "url": "http://localhost:8080"
    }
  }
}
```

## Available tools

| Tool | Required args | Optional args |
|------|--------------|---------------|
| `get_sales_metrics` | `period` (`day` \| `month` \| `year`) | `compare_to_previous` (bool) |
| `get_top_products` | — | `limit` (int, default 5) |
| `get_financial_status` | `category` (`cash_flow` \| `expenses` \| `profit_margin` \| `budget_vs_actual`) | — |
| `get_customer_metrics` | `metric` (`active_customers` \| `churn_rate` \| `sales_pipeline`) | — |
| `get_project_status` | — | `department` (string) |
| `get_team_metrics` | `metric` (`headcount` \| `vacancies`) | — |
| `get_product_health` | `metric` (`active_users` \| `uptime` \| `open_tickets`) | — |
| `generate_executive_summary` | — | `departments` (string array) |
| `get_anomalies` | — | `threshold` (number, default 20%) |
| `get_inventory_levels` | — | `category` (string) |

## Dev shell (Nix)

```bash
nix develop
```

Provides `cargo`, `rust-analyzer`, `clippy`, `rustfmt`, `bacon`, and `claude-code`.
