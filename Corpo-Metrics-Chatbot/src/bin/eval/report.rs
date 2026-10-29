//! Aggregates per-case results into metrics and writes the run's files.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::{CaseResult, RunConfig};

#[derive(Debug, Default)]
struct Metrics {
    cases: usize,
    errors: usize,
    pass_rate: f64,
    tool_recall: Option<f64>,
    tool_precision: Option<f64>,
    tool_f1: Option<f64>,
    args_accuracy: Option<f64>,
    fact_recall: Option<f64>,
    grounded_rate: Option<f64>,
    decline_accuracy: Option<f64>,
    unsupported_number_rate: Option<f64>,
    latency_mean: f64,
    latency_p50: f64,
    latency_p95: f64,
    input_tokens_mean: f64,
    output_tokens_mean: f64,
    api_calls_mean: f64,
    cost_mean: f64,
    cost_total: f64,
}

fn mean(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let values: Vec<f64> = values.into_iter().collect();
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn compute(results: &[CaseResult]) -> Metrics {
    let ok: Vec<&CaseResult> = results.iter().filter(|r| r.error.is_none()).collect();
    let rate = |hits: usize, total: usize| (total > 0).then(|| hits as f64 / total as f64);

    let tool_recall = mean(ok.iter().filter_map(|r| r.tool_recall));
    let tool_precision = mean(ok.iter().filter_map(|r| r.tool_precision));
    let tool_f1 = match (tool_recall, tool_precision) {
        (Some(r), Some(p)) if r + p > 0.0 => Some(2.0 * p * r / (p + r)),
        (Some(_), Some(_)) => Some(0.0),
        _ => None,
    };

    let args: Vec<bool> = ok.iter().filter_map(|r| r.args_ok).collect();
    let facts_expected: usize = ok.iter().map(|r| r.facts_expected).sum();
    let facts_found: usize = ok.iter().map(|r| r.facts_found).sum();
    let judged: Vec<bool> = ok.iter().filter_map(|r| r.judge.as_ref().map(|v| v.grounded)).collect();
    let declines: Vec<bool> = ok
        .iter()
        .filter(|r| r.should_decline)
        .filter_map(|r| r.judge.as_ref().map(|v| v.declined))
        .collect();
    let numbers: usize = ok.iter().map(|r| r.numbers_in_answer).sum();
    let unsupported: usize = ok.iter().map(|r| r.unsupported_numbers.len()).sum();

    let mut latencies: Vec<f64> = ok.iter().map(|r| r.latency_secs).collect();
    latencies.sort_by(f64::total_cmp);

    Metrics {
        cases: results.len(),
        errors: results.len() - ok.len(),
        pass_rate: rate(results.iter().filter(|r| r.passed).count(), results.len()).unwrap_or(0.0),
        tool_recall,
        tool_precision,
        tool_f1,
        args_accuracy: rate(args.iter().filter(|&&a| a).count(), args.len()),
        fact_recall: rate(facts_found, facts_expected),
        grounded_rate: rate(judged.iter().filter(|&&g| g).count(), judged.len()),
        decline_accuracy: rate(declines.iter().filter(|&&d| d).count(), declines.len()),
        unsupported_number_rate: rate(unsupported, numbers),
        latency_mean: mean(latencies.iter().copied()).unwrap_or(0.0),
        latency_p50: percentile(&latencies, 0.5),
        latency_p95: percentile(&latencies, 0.95),
        input_tokens_mean: mean(ok.iter().map(|r| r.input_tokens as f64)).unwrap_or(0.0),
        output_tokens_mean: mean(ok.iter().map(|r| r.output_tokens as f64)).unwrap_or(0.0),
        api_calls_mean: mean(ok.iter().map(|r| r.api_calls as f64)).unwrap_or(0.0),
        cost_mean: mean(ok.iter().map(|r| r.cost_usd)).unwrap_or(0.0),
        cost_total: results.iter().map(|r| r.cost_usd).sum(),
    }
}

fn pct(v: Option<f64>) -> String {
    v.map(|x| format!("{:.1}%", x * 100.0)).unwrap_or_else(|| "—".to_string())
}

fn dec(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.3}")).unwrap_or_else(|| "—".to_string())
}

/// Writes results.jsonl, summary.md, judge_review.csv and appends to
/// iterations.csv. Returns the summary markdown.
pub fn write(
    dir: &Path,
    out_root: &Path,
    run: &RunConfig,
    results: &[CaseResult],
    judge_cost: f64,
) -> Result<String> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let m = compute(results);
    let date = today();

    let mut jsonl = String::new();
    for r in results {
        jsonl.push_str(&serde_json::to_string(r)?);
        jsonl.push('\n');
    }
    fs::write(dir.join("results.jsonl"), jsonl)?;

    let mut md = String::new();
    writeln!(md, "# Evaluation run `{}`\n", run.label)?;
    writeln!(md, "| Setting | Value |\n|---|---|")?;
    writeln!(md, "| Date | {date} |")?;
    writeln!(md, "| Model | `{}` |", run.model)?;
    writeln!(md, "| System prompt | {} |", run.system_prompt_name)?;
    writeln!(md, "| MCP tools | {} |", if run.tools { "on" } else { "off" })?;
    writeln!(md, "| Judge | {} |", run.judge_model.as_deref().unwrap_or("off"))?;
    if !run.notes.is_empty() {
        writeln!(md, "| Notes | {} |", run.notes)?;
    }
    writeln!(md, "\n## Quality\n\n| Metric | Value |\n|---|---|")?;
    writeln!(md, "| Cases (errors) | {} ({}) |", m.cases, m.errors)?;
    writeln!(md, "| **Task success (pass rate)** | **{}** |", pct(Some(m.pass_rate)))?;
    writeln!(md, "| Tool selection recall / precision / F1 | {} / {} / {} |", dec(m.tool_recall), dec(m.tool_precision), dec(m.tool_f1))?;
    writeln!(md, "| Argument accuracy | {} |", pct(m.args_accuracy))?;
    writeln!(md, "| Fact recall (expected figures/names in answer) | {} |", pct(m.fact_recall))?;
    writeln!(md, "| Faithfulness (judge: grounded answers) | {} |", pct(m.grounded_rate))?;
    writeln!(md, "| Hallucination rate (1 − faithfulness) | {} |", pct(m.grounded_rate.map(|g| 1.0 - g)))?;
    writeln!(md, "| Correct declines on out-of-scope questions | {} |", pct(m.decline_accuracy))?;
    writeln!(md, "| Numbers not traceable to a tool output (heuristic) | {} |", pct(m.unsupported_number_rate))?;
    writeln!(md, "\n## Operational\n\n| Metric | Value |\n|---|---|")?;
    writeln!(md, "| Latency mean / p50 / p95 | {:.2} s / {:.2} s / {:.2} s |", m.latency_mean, m.latency_p50, m.latency_p95)?;
    writeln!(md, "| Tokens per query (input / output) | {:.0} / {:.0} |", m.input_tokens_mean, m.output_tokens_mean)?;
    writeln!(md, "| API calls per query | {:.2} |", m.api_calls_mean)?;
    writeln!(md, "| Cost per query | ${:.4} |", m.cost_mean)?;
    writeln!(md, "| Total cost (assistant / judge) | ${:.3} / ${:.3} |", m.cost_total, judge_cost)?;

    let mut by_category: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for r in results {
        let entry = by_category.entry(r.category.as_str()).or_default();
        entry.0 += 1;
        entry.1 += usize::from(r.passed);
    }
    writeln!(md, "\n## Pass rate by category\n\n| Category | Cases | Passed | Rate |\n|---|---|---|---|")?;
    for (category, (n, passed)) in &by_category {
        writeln!(md, "| {category} | {n} | {passed} | {:.1}% |", *passed as f64 * 100.0 / *n as f64)?;
    }

    let failures: Vec<&CaseResult> = results.iter().filter(|r| !r.passed).collect();
    if !failures.is_empty() {
        writeln!(md, "\n## Failed cases\n\n| Case | Question | Why |\n|---|---|---|")?;
        for r in failures {
            writeln!(md, "| {} | {} | {} |", r.id, r.question.replace('|', "/"), failure_summary(r).replace('|', "/"))?;
        }
    }
    fs::write(dir.join("summary.md"), &md)?;

    write_judge_review(dir, results)?;
    append_iteration(out_root, run, &m, &date)?;
    Ok(md)
}

fn failure_summary(r: &CaseResult) -> String {
    if let Some(e) = &r.error {
        return format!("error: {e}");
    }
    let mut parts = Vec::new();
    if r.tool_recall.is_some_and(|x| x < 1.0) {
        let called: Vec<_> = r.tool_calls.iter().map(|c| c.name.as_str()).collect();
        parts.push(format!("wrong/missing tools (called {called:?})"));
    }
    if !r.missing_facts.is_empty() {
        parts.push(format!("missing: {}", r.missing_facts.join(", ")));
    }
    if let Some(v) = &r.judge {
        if !v.grounded {
            parts.push(format!("unsupported: {}", v.unsupported_claims.join("; ")));
        }
        if r.should_decline && !v.declined {
            parts.push("answered instead of declining".to_string());
        }
    }
    parts.join(" · ")
}

fn csv_field(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn write_judge_review(dir: &Path, results: &[CaseResult]) -> Result<()> {
    let mut csv = String::from(
        "id,category,question,answer,judge_grounded,judge_declined,judge_unsupported_claims,human_grounded,human_declined,human_notes\n",
    );
    for r in results {
        let Some(v) = &r.judge else { continue };
        let row = [
            csv_field(&r.id),
            csv_field(&r.category),
            csv_field(&r.question),
            csv_field(&r.answer),
            v.grounded.to_string(),
            v.declined.to_string(),
            csv_field(&v.unsupported_claims.join("; ")),
            String::new(),
            String::new(),
            String::new(),
        ];
        csv.push_str(&row.join(","));
        csv.push('\n');
    }
    fs::write(dir.join("judge_review.csv"), csv)?;
    Ok(())
}

fn append_iteration(out_root: &Path, run: &RunConfig, m: &Metrics, date: &str) -> Result<()> {
    let path = out_root.join("iterations.csv");
    let is_new = !path.exists();
    let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    if is_new {
        writeln!(
            file,
            "label,date,model,system_prompt,tools,cases,errors,pass_rate,tool_recall,tool_precision,tool_f1,args_accuracy,fact_recall,grounded_rate,decline_accuracy,unsupported_number_rate,latency_mean_s,latency_p95_s,input_tokens_mean,output_tokens_mean,api_calls_mean,cost_per_query_usd,notes"
        )?;
    }
    let opt = |v: Option<f64>| v.map(|x| format!("{x:.4}")).unwrap_or_default();
    writeln!(
        file,
        "{},{},{},{},{},{},{},{:.4},{},{},{},{},{},{},{},{},{:.3},{:.3},{:.0},{:.0},{:.2},{:.5},{}",
        csv_field(&run.label),
        date,
        run.model,
        csv_field(&run.system_prompt_name),
        run.tools,
        m.cases,
        m.errors,
        m.pass_rate,
        opt(m.tool_recall),
        opt(m.tool_precision),
        opt(m.tool_f1),
        opt(m.args_accuracy),
        opt(m.fact_recall),
        opt(m.grounded_rate),
        opt(m.decline_accuracy),
        opt(m.unsupported_number_rate),
        m.latency_mean,
        m.latency_p95,
        m.input_tokens_mean,
        m.output_tokens_mean,
        m.api_calls_mean,
        m.cost_mean,
        csv_field(&run.notes),
    )?;
    Ok(())
}

/// UTC date as YYYY-MM-DD without pulling in a date crate.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Civil-from-days (H. Hinnant).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Minimal RFC 4180 reader (quoted fields, doubled quotes, embedded newlines).
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, in_quotes) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => in_quotes = !in_quotes,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            ('\r', false) => {}
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn parse_label(s: &str) -> Option<bool> {
    match s.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "y" | "si" | "sí" | "s" => Some(true),
        "false" | "0" | "no" | "n" => Some(false),
        _ => None,
    }
}

/// Agreement between judge verdicts and human labels in a filled-in
/// `judge_review.csv` (percent agreement and Cohen's kappa).
pub fn agreement(path: &str) -> Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    let rows = parse_csv(&text);
    let Some((header, rows)) = rows.split_first() else {
        bail!("{path} is empty");
    };
    let col = |name: &str| {
        header
            .iter()
            .position(|h| h == name)
            .with_context(|| format!("column {name} not found"))
    };

    for (judge_col, human_col, label) in [
        ("judge_grounded", "human_grounded", "grounded"),
        ("judge_declined", "human_declined", "declined"),
    ] {
        let (j, h) = (col(judge_col)?, col(human_col)?);
        let pairs: Vec<(bool, bool)> = rows
            .iter()
            .filter_map(|r| Some((parse_label(r.get(j)?)?, parse_label(r.get(h)?)?)))
            .collect();
        if pairs.is_empty() {
            println!("{label}: no human labels yet (fill the {human_col} column)");
            continue;
        }
        let n = pairs.len() as f64;
        let agree = pairs.iter().filter(|(a, b)| a == b).count() as f64 / n;
        let p_judge = pairs.iter().filter(|(a, _)| *a).count() as f64 / n;
        let p_human = pairs.iter().filter(|(_, b)| *b).count() as f64 / n;
        let expected = p_judge * p_human + (1.0 - p_judge) * (1.0 - p_human);
        let kappa = if (1.0 - expected).abs() < 1e-12 {
            1.0
        } else {
            (agree - expected) / (1.0 - expected)
        };
        println!(
            "{label}: {} labelled cases · agreement {:.1}% · Cohen's kappa {:.3}",
            pairs.len(),
            agree * 100.0,
            kappa
        );
    }
    Ok(())
}
