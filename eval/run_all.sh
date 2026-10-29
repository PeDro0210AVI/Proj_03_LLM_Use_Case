#!/usr/bin/env bash
# Reproduces every evaluation iteration reported in docs/REPORTE.md.
# Run from the repository root. Needs ANTHROPIC_API_KEY (or
# Corpo-Metrics-Chatbot/.env) and spends API credits (≈ $4–6 for all runs).
set -euo pipefail

cargo build --workspace --release

# Start the MCP server unless one is already listening.
if ! curl -s -o /dev/null -X POST -H 'Content-Type: application/json' \
     -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' http://localhost:8080; then
  (cd Corpo-Metrics && ../target/release/corpo_metrics > /dev/null 2>&1 &)
  trap 'pkill -f target/release/corpo_metrics || true' EXIT
  sleep 1
fi

EVAL=./target/release/eval
MODEL=${MODEL:-claude-sonnet-5}

# I0 — baseline: same model and prompt, no MCP tools (pure LLM).
$EVAL --label i0-no-tools --model "$MODEL" --no-tools \
      --notes "Baseline: LLM without tools"
# I1 — tools, no system prompt.
$EVAL --label i1-tools-no-prompt --model "$MODEL" --system-prompt none \
      --notes "Tools without system prompt"
# I2 — tools + system prompt v2 (the shipped product).
$EVAL --label i2-tools-prompt-v2 --model "$MODEL" \
      --system-prompt Corpo-Metrics-Chatbot/prompts/system_v2.md \
      --notes "Product configuration"
# I3 — smaller model, same configuration as I2.
$EVAL --label i3-haiku-prompt-v2 --model claude-haiku-5-5 \
      --system-prompt Corpo-Metrics-Chatbot/prompts/system_v2.md \
      --notes "Smaller model"

echo
echo "Iteration table: eval/results/iterations.csv"
