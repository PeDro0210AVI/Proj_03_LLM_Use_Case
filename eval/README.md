# Evaluation

Measures how well the chatbot answers business-metric questions, using the
same `ClaudeClient` + MCP tool loop as the TUI.

| File | What it is |
|---|---|
| `cases.json` | 50 cases: question, expected tools (groups of acceptable alternatives), expected arguments, expected figures/names, `should_decline` |
| `run_all.sh` | Reproduces every iteration in the report (I0–I3) |
| `results/<label>/summary.md` | Metrics for one run, plus the failed cases |
| `results/<label>/results.jsonl` | Per case: tool calls, tool outputs, answer, judge verdict, tokens, latency, cost |
| `results/<label>/judge_review.csv` | Judge verdicts with blank `human_*` columns for validating the judge |
| `results/iterations.csv` | One row per run: the iteration table |

The harness lives in `Corpo-Metrics-Chatbot/src/bin/eval/`.

## Running

From the repository root, with `ANTHROPIC_API_KEY` set (or in
`Corpo-Metrics-Chatbot/.env`) and the MCP server running:

```sh
./eval/run_all.sh                       # all iterations (≈ $4–6 of API usage)

# or one run
cargo run --release -p corpo-metrics-chatbot --bin eval -- \
  --label my-run [--model claude-haiku-5-5] [--system-prompt none|PATH] \
  [--no-tools] [--no-judge] [--only S01,M03] [--notes "what changed"]
```

`--only` and `--no-judge` are handy for cheap smoke runs while iterating.

## Metrics

- **Pass rate.** A case passes when every required tool group was called,
  every expected figure and name appears in the answer, and the judge marks
  the answer grounded. Out-of-scope cases pass when the judge confirms the
  answer declined and made no unsupported claims.
- **Tool recall / precision / F1** and **argument accuracy.**
- **Fact recall.** Figures are matched by `numbers.rs`, which understands
  `287,640`, `287.640`, `$287.6K` and `2,14 millones`, and accepts any value
  consistent with the precision the answer wrote.
- **Faithfulness / hallucination rate.** From the LLM judge
  (`claude-sonnet-5-5`, structured JSON output).
- **Decline accuracy** on out-of-scope questions.
- **Numbers not traceable to any tool output.** A deterministic heuristic.
  It also flags correct derived numbers, so treat it as a secondary metric.
- **Latency** (mean, p50, p95), **tokens** in/out, **API calls** and **cost**
  per query.

## Validating the judge against humans

1. Open `results/<label>/judge_review.csv` and fill `human_grounded` /
   `human_declined` (`true`/`false`, `sí`/`no`) for at least 20 rows.
2. Run:

   ```sh
   cargo run -p corpo-metrics-chatbot --bin eval -- agreement eval/results/<label>/judge_review.csv
   ```

   It prints percent agreement and Cohen's kappa.

## Tests

```sh
cargo test -p corpo-metrics-chatbot --bin eval   # number parser + scoring, no API calls
```
