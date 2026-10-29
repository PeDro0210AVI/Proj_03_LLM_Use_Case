//! Evaluation harness for the Corpo Metrics chatbot.
//!
//! Runs every case in `eval/cases.json` through the same `ClaudeClient` +
//! MCP tool loop the TUI uses, scores the answers and writes:
//!
//! - `eval/results/<label>/results.jsonl`    one line per case, full detail
//! - `eval/results/<label>/summary.md`       metrics for this run
//! - `eval/results/<label>/judge_review.csv` judge verdicts + blank columns
//!   for human labels
//! - `eval/results/iterations.csv`           one row appended per run
//!
//! Run from the repository root with the MCP server up:
//!
//! ```sh
//! cargo run -p corpo-metrics-chatbot --bin eval -- --label i2-tools-prompt
//! cargo run -p corpo-metrics-chatbot --bin eval -- agreement eval/results/<label>/judge_review.csv
//! ```

mod judge;
mod numbers;
mod report;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc::unbounded_channel;

use corpo_metrics_chatbot::claude::{ApiEvent, ClaudeClient};
use corpo_metrics_chatbot::config::Config;
use corpo_metrics_chatbot::mcp::McpClient;
use corpo_metrics_chatbot::pricing::Usage;

use judge::{Judge, Verdict};

const DEFAULT_JUDGE_MODEL: &str = "claude-sonnet-5-5";
/// Attempts per case when the API errors out (overload, rate limit, ...).
const MAX_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Deserialize)]
pub struct Case {
    pub id: String,
    pub category: String,
    #[serde(default)]
    pub lang: String,
    pub question: String,
    /// Each inner list is one requirement satisfied by calling any of its
    /// tools. `None` means "no tool requirement" (out-of-scope, gibberish).
    #[serde(default)]
    pub expected_tools: Option<Vec<Vec<String>>>,
    /// Tool name → argument subset that at least one call must match.
    #[serde(default)]
    pub expected_args: BTreeMap<String, Value>,
    #[serde(default)]
    pub expected_numbers: Vec<f64>,
    /// Each inner list is one fact; any alternative (case-insensitive
    /// substring) satisfies it.
    #[serde(default)]
    pub expected_text: Vec<Vec<String>>,
    #[serde(default)]
    pub should_decline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CaseResult {
    pub id: String,
    pub category: String,
    pub question: String,
    pub answer: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_outputs: Vec<String>,
    pub error: Option<String>,
    pub latency_secs: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub api_calls: u32,
    pub cost_usd: f64,
    pub tool_recall: Option<f64>,
    pub tool_precision: Option<f64>,
    pub args_ok: Option<bool>,
    pub facts_expected: usize,
    pub facts_found: usize,
    pub missing_facts: Vec<String>,
    pub numbers_in_answer: usize,
    pub unsupported_numbers: Vec<String>,
    pub should_decline: bool,
    pub judge: Option<Verdict>,
    pub judge_error: Option<String>,
    pub passed: bool,
}

pub struct RunConfig {
    pub label: String,
    pub model: String,
    pub system_prompt_name: String,
    pub tools: bool,
    pub judge_model: Option<String>,
    pub notes: String,
}

struct Args {
    label: String,
    model: Option<String>,
    system_prompt: Option<Option<PathBuf>>,
    tools: bool,
    judge_model: Option<String>,
    only: Option<Vec<String>>,
    cases: PathBuf,
    out: PathBuf,
    notes: String,
}

fn usage_text() -> &'static str {
    "usage: eval --label NAME [--model ID] [--system-prompt PATH|none] [--no-tools]\n\
     \x20           [--judge-model ID | --no-judge] [--only ID,ID] [--cases PATH]\n\
     \x20           [--out DIR] [--notes TEXT]\n\
     \x20      eval agreement PATH/TO/judge_review.csv"
}

fn parse_args(raw: Vec<String>) -> Result<Args> {
    let mut args = Args {
        label: String::new(),
        model: None,
        system_prompt: None,
        tools: true,
        judge_model: Some(DEFAULT_JUDGE_MODEL.to_string()),
        only: None,
        cases: PathBuf::from("eval/cases.json"),
        out: PathBuf::from("eval/results"),
        notes: String::new(),
    };
    let mut it = raw.into_iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--label" => args.label = value()?,
            "--model" => args.model = Some(value()?),
            "--system-prompt" => {
                let v = value()?;
                args.system_prompt = Some((v != "none").then(|| PathBuf::from(v)));
            }
            "--no-tools" => args.tools = false,
            "--judge-model" => args.judge_model = Some(value()?),
            "--no-judge" => args.judge_model = None,
            "--only" => args.only = Some(value()?.split(',').map(str::to_string).collect()),
            "--cases" => args.cases = PathBuf::from(value()?),
            "--out" => args.out = PathBuf::from(value()?),
            "--notes" => args.notes = value()?,
            "-h" | "--help" => {
                println!("{}", usage_text());
                std::process::exit(0);
            }
            other => bail!("unknown argument {other:?}\n{}", usage_text()),
        }
    }
    if args.label.is_empty() {
        bail!("--label is required\n{}", usage_text());
    }
    Ok(args)
}

#[tokio::main]
async fn main() -> Result<()> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().map(String::as_str) == Some("agreement") {
        let path = raw.get(1).context("usage: eval agreement PATH/TO/judge_review.csv")?;
        return report::agreement(path);
    }
    let args = parse_args(raw)?;

    // Run from the repo root, where the chatbot's .env isn't in scope.
    let _ = dotenvy::from_filename("Corpo-Metrics-Chatbot/.env");
    let config = Config::load()?;

    let cases: Vec<Case> = serde_json::from_str(
        &std::fs::read_to_string(&args.cases)
            .with_context(|| format!("reading {}", args.cases.display()))?,
    )
    .context("parsing cases file")?;
    let cases: Vec<Case> = match &args.only {
        Some(ids) => cases.into_iter().filter(|c| ids.contains(&c.id)).collect(),
        None => cases,
    };

    let mcp = McpClient::connect(&config.mcp_server_url, config.mcp_auth_token.clone())
        .await
        .with_context(|| format!("connecting to the MCP server at {}", config.mcp_server_url))?;

    let (system_prompt, system_prompt_name) = match &args.system_prompt {
        None => (
            Some(corpo_metrics_chatbot::claude::DEFAULT_SYSTEM_PROMPT.trim().to_string()),
            "prompts/system_v2.md (default)".to_string(),
        ),
        Some(None) => (None, "none".to_string()),
        Some(Some(path)) => (
            Some(
                std::fs::read_to_string(path)
                    .with_context(|| format!("reading {}", path.display()))?
                    .trim()
                    .to_string(),
            ),
            path.display().to_string(),
        ),
    };

    let mut client = ClaudeClient::new(&config)
        .with_system_prompt(system_prompt)
        .with_tools(args.tools);
    if let Some(model) = &args.model {
        client = client.with_model(model.clone());
    }
    let judge = args
        .judge_model
        .clone()
        .map(|model| Judge::new(config.api_key.clone(), model));

    let run = RunConfig {
        label: args.label.clone(),
        model: client.model().to_string(),
        system_prompt_name,
        tools: args.tools,
        judge_model: args.judge_model.clone(),
        notes: args.notes.clone(),
    };

    println!(
        "eval '{}': {} cases · model {} · tools {} · prompt {} · judge {}",
        run.label,
        cases.len(),
        run.model,
        if run.tools { "on" } else { "off" },
        run.system_prompt_name,
        run.judge_model.as_deref().unwrap_or("off"),
    );

    let mut results = Vec::with_capacity(cases.len());
    let mut judge_usage = Usage::default();
    for (i, case) in cases.iter().enumerate() {
        let mut result = run_case(&client, &mcp, case).await;
        score(case, &mut result, run.tools);

        if let (Some(judge), None) = (&judge, &result.error) {
            match judge.grade(&case.question, &result.tool_outputs, &result.answer).await {
                Ok((verdict, usage)) => {
                    judge_usage.add(&usage);
                    result.judge = Some(verdict);
                }
                Err(e) => result.judge_error = Some(format!("{e:#}")),
            }
        }
        result.passed = passed(case, &result, run.tools);

        println!(
            "[{:>2}/{}] {:<4} {:<4} {:>5.1}s  {}",
            i + 1,
            cases.len(),
            case.id,
            if result.passed { "PASS" } else { "FAIL" },
            result.latency_secs,
            failure_reason(&result).unwrap_or_default(),
        );
        results.push(result);
    }

    let judge_cost = run
        .judge_model
        .as_deref()
        .and_then(|m| judge_usage.cost_usd(m))
        .unwrap_or(0.0);
    let dir = args.out.join(&run.label);
    let summary = report::write(&dir, &args.out, &run, &results, judge_cost)?;
    println!("\n{summary}");
    println!("results written to {}", dir.display());
    Ok(())
}

/// Runs one case through the real tool loop, retrying transient API errors.
async fn run_case(client: &ClaudeClient, mcp: &McpClient, case: &Case) -> CaseResult {
    let mut result = CaseResult {
        id: case.id.clone(),
        category: case.category.clone(),
        question: case.question.clone(),
        should_decline: case.should_decline,
        ..Default::default()
    };

    for attempt in 1..=MAX_ATTEMPTS {
        let (tx, mut rx) = unbounded_channel::<ApiEvent>();
        let history = vec![json!({"role": "user", "content": case.question})];
        client.run_turn(history, mcp, tx).await;

        let mut outcome = None;
        while let Some(event) = rx.recv().await {
            match event {
                ApiEvent::Done { new_messages, usage } => outcome = Some(Ok((new_messages, usage))),
                ApiEvent::Error(e) => outcome = Some(Err(e)),
                _ => {}
            }
        }

        match outcome {
            Some(Ok((messages, usage))) => {
                fill_from_transcript(&mut result, &messages);
                result.latency_secs = usage.elapsed_secs;
                result.input_tokens = usage.total_input_tokens();
                result.output_tokens = usage.output_tokens;
                result.api_calls = usage.api_calls;
                result.cost_usd = usage.cost_usd(client.model()).unwrap_or(0.0);
                result.error = None;
                return result;
            }
            Some(Err(e)) => result.error = Some(e),
            None => result.error = Some("turn ended without a result".to_string()),
        }
        if attempt < MAX_ATTEMPTS {
            tokio::time::sleep(Duration::from_secs(5 * attempt as u64)).await;
        }
    }
    result
}

/// Pulls tool calls, tool outputs and the final answer out of the
/// messages a turn produced.
fn fill_from_transcript(result: &mut CaseResult, messages: &[Value]) {
    for message in messages {
        let blocks = message["content"].as_array().cloned().unwrap_or_default();
        for block in &blocks {
            match block["type"].as_str() {
                Some("tool_use") => result.tool_calls.push(ToolCall {
                    name: block["name"].as_str().unwrap_or("").to_string(),
                    input: block["input"].clone(),
                }),
                Some("tool_result") => result
                    .tool_outputs
                    .push(block["content"].as_str().unwrap_or("").to_string()),
                _ => {}
            }
        }
    }
    // The answer is the text of the last assistant message; earlier text
    // is preamble before tool calls.
    if let Some(last) = messages.iter().rev().find(|m| m["role"] == "assistant") {
        result.answer = last["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
    }
}

/// Deterministic metrics: tool selection, arguments, expected facts and
/// numbers that can't be traced back to any tool output.
fn score(case: &Case, result: &mut CaseResult, tools_enabled: bool) {
    if result.error.is_some() {
        return;
    }

    if let (Some(groups), true) = (&case.expected_tools, tools_enabled) {
        let called: Vec<&str> = result.tool_calls.iter().map(|c| c.name.as_str()).collect();
        let satisfied = groups
            .iter()
            .filter(|g| g.iter().any(|t| called.contains(&t.as_str())))
            .count();
        result.tool_recall = Some(satisfied as f64 / groups.len().max(1) as f64);
        let relevant = called
            .iter()
            .filter(|name| groups.iter().flatten().any(|t| t == *name))
            .count();
        result.tool_precision = Some(if called.is_empty() {
            0.0
        } else {
            relevant as f64 / called.len() as f64
        });
    }

    if tools_enabled && !case.expected_args.is_empty() {
        result.args_ok = Some(case.expected_args.iter().all(|(tool, want)| {
            result
                .tool_calls
                .iter()
                .any(|c| c.name == *tool && args_contain(&c.input, want))
        }));
    }

    let answer_numbers = numbers::extract(&result.answer);
    let answer_lower = result.answer.to_lowercase();
    result.facts_expected = case.expected_numbers.len() + case.expected_text.len();
    for expected in &case.expected_numbers {
        if answer_numbers.iter().any(|n| n.matches(*expected)) {
            result.facts_found += 1;
        } else {
            result.missing_facts.push(format_number(*expected));
        }
    }
    for alternatives in &case.expected_text {
        if alternatives.iter().any(|a| answer_lower.contains(&a.to_lowercase())) {
            result.facts_found += 1;
        } else {
            result.missing_facts.push(alternatives.join(" | "));
        }
    }

    // Numbers the answer states that appear in no tool output (nor the
    // question). Heuristic: correct derived figures (ratios, sums) also
    // land here, which is why the judge is the primary faithfulness metric.
    let mut pool: Vec<f64> = result
        .tool_outputs
        .iter()
        .chain(std::iter::once(&case.question))
        .flat_map(|t| numbers::extract(t))
        .flat_map(|n| n.readings.into_iter().map(|(v, _)| v))
        .collect();
    pool.sort_by(f64::total_cmp);
    let substantive: Vec<_> = answer_numbers.iter().filter(|n| !n.is_trivial()).collect();
    result.numbers_in_answer = substantive.len();
    result.unsupported_numbers = substantive
        .iter()
        .filter(|n| !pool.iter().any(|&v| n.matches(v)))
        .map(|n| n.raw.clone())
        .collect();
}

fn args_contain(actual: &Value, want: &Value) -> bool {
    match (actual, want) {
        (Value::Object(a), Value::Object(w)) => w
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|av| args_contain(av, v))),
        (Value::String(a), Value::String(w)) => a.eq_ignore_ascii_case(w),
        _ => actual == want,
    }
}

const DECLINE_CUES: &[&str] = &[
    "no tengo", "no cuento", "no dispongo", "no está disponible", "no esta disponible",
    "no hay datos", "no hay información", "no puedo", "no incluye", "no existe",
    "not available", "don't have", "do not have", "can't", "cannot", "no data",
];

fn passed(case: &Case, result: &CaseResult, tools_enabled: bool) -> bool {
    if result.error.is_some() {
        return false;
    }
    let grounded = result.judge.as_ref().map(|v| v.grounded);
    if case.should_decline {
        let declined = match &result.judge {
            Some(v) => v.declined,
            None => {
                let lower = result.answer.to_lowercase();
                DECLINE_CUES.iter().any(|c| lower.contains(c))
            }
        };
        return declined && grounded.unwrap_or(true);
    }
    let tools_ok = !tools_enabled || result.tool_recall.is_none_or(|r| r >= 1.0);
    tools_ok && result.facts_found == result.facts_expected && grounded.unwrap_or(true)
}

fn failure_reason(result: &CaseResult) -> Option<String> {
    if result.passed {
        return None;
    }
    if let Some(e) = &result.error {
        return Some(format!("error: {}", e.chars().take(80).collect::<String>()));
    }
    let mut reasons = Vec::new();
    if result.tool_recall.is_some_and(|r| r < 1.0) {
        let called: Vec<_> = result.tool_calls.iter().map(|c| c.name.as_str()).collect();
        reasons.push(format!("tools {called:?}"));
    }
    if !result.missing_facts.is_empty() {
        reasons.push(format!("missing {}", result.missing_facts.join(", ")));
    }
    if let Some(v) = &result.judge {
        if !v.grounded {
            reasons.push("ungrounded".to_string());
        }
        if result.should_decline && !v.declined {
            reasons.push("did not decline".to_string());
        }
    }
    Some(reasons.join("; "))
}

pub fn format_number(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(value: Value) -> Case {
        serde_json::from_value(value).unwrap()
    }

    /// A transcript shaped like the one `ClaudeClient::run_turn` returns.
    fn transcript(tool: &str, input: Value, output: &str, answer: &str) -> Vec<Value> {
        vec![
            json!({"role": "assistant", "content": [
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "t1", "name": tool, "input": input}
            ]}),
            json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": output, "is_error": false}
            ]}),
            json!({"role": "assistant", "content": [{"type": "text", "text": answer}]}),
        ]
    }

    fn evaluate(case: &Case, messages: &[Value]) -> CaseResult {
        let mut result = CaseResult::default();
        fill_from_transcript(&mut result, messages);
        score(case, &mut result, true);
        result.passed = passed(case, &result, true);
        result
    }

    const SALES_MONTH: &str = "Sales Metrics (month):\n  Revenue:     $287640.00\n  Orders:      1082";

    #[test]
    fn correct_answer_passes() {
        let c = case(json!({"id": "S01", "category": "single_tool", "question": "¿Cuánto vendimos este mes?",
            "expected_tools": [["get_sales_metrics", "generate_executive_summary"]],
            "expected_args": {"get_sales_metrics": {"period": "month"}},
            "expected_numbers": [287640]}));
        let r = evaluate(&c, &transcript("get_sales_metrics", json!({"period": "month"}), SALES_MONTH,
            "Este mes vendimos **$287,640** en 1,082 órdenes."));
        assert_eq!(r.answer, "Este mes vendimos **$287,640** en 1,082 órdenes.");
        assert_eq!(r.tool_recall, Some(1.0));
        assert_eq!(r.tool_precision, Some(1.0));
        assert_eq!(r.args_ok, Some(true));
        assert_eq!(r.facts_found, 1);
        assert!(r.unsupported_numbers.is_empty(), "{:?}", r.unsupported_numbers);
        assert!(r.passed);
    }

    #[test]
    fn wrong_figure_and_args_fail() {
        let c = case(json!({"id": "S01", "category": "single_tool", "question": "¿Cuánto vendimos este mes?",
            "expected_tools": [["get_sales_metrics"]],
            "expected_args": {"get_sales_metrics": {"period": "month"}},
            "expected_numbers": [287640]}));
        let r = evaluate(&c, &transcript("get_sales_metrics", json!({"period": "year"}), SALES_MONTH,
            "Vendimos $300,000 este mes."));
        assert_eq!(r.args_ok, Some(false));
        assert_eq!(r.facts_found, 0);
        assert_eq!(r.unsupported_numbers, vec!["300,000".to_string()]);
        assert!(!r.passed);
    }

    #[test]
    fn decline_falls_back_to_keywords_without_judge() {
        let c = case(json!({"id": "O03", "category": "out_of_scope",
            "question": "¿Cuál es el NPS de nuestros clientes?", "should_decline": true}));
        let messages = vec![json!({"role": "assistant", "content": [
            {"type": "text", "text": "No tengo datos de NPS en las herramientas disponibles."}]})];
        assert!(evaluate(&c, &messages).passed);
    }
}
