//! LLM-as-a-judge: checks whether an answer is grounded in the tool outputs
//! the assistant actually saw, and whether it declined when it should have.
//! Its verdicts are exported to `judge_review.csv` so a human can label a
//! sample and measure agreement (`eval agreement <csv>`).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use corpo_metrics_chatbot::pricing::Usage;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

const JUDGE_SYSTEM: &str = "\
You grade an AI assistant that answers questions about a company's business \
metrics using tools. You receive the user's question, the exact tool outputs \
the assistant saw (possibly none) and the assistant's final answer.

Decide:
- grounded: true if every factual claim about the company in the answer \
(numbers, names, dates, statuses) is supported by the tool outputs, or is a \
correct arithmetic derivation from them (sums, differences, ratios, \
percentages, simple projections presented as such). Reasonable rounding is \
fine. Statements that information is unavailable, clarifying questions and \
generic advice are not claims. If there are no tool outputs, any specific \
company figure is unsupported.
- unsupported_claims: each unsupported claim, quoted briefly. Empty if grounded.
- declined: true if the answer explicitly says the specific information \
requested is not available or cannot be answered with the available data, \
instead of providing it. Offering related data that does exist is fine as \
long as it is clear the thing asked for is not available.

Write a short reasoning first, then the verdicts.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub reasoning: String,
    pub grounded: bool,
    pub unsupported_claims: Vec<String>,
    pub declined: bool,
}

pub struct Judge {
    http: reqwest::Client,
    api_key: String,
    pub model: String,
}

impl Judge {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            model,
        }
    }

    pub async fn grade(
        &self,
        question: &str,
        tool_outputs: &[String],
        answer: &str,
    ) -> Result<(Verdict, Usage)> {
        let outputs = if tool_outputs.is_empty() {
            "(none — the assistant did not call any tool)".to_string()
        } else {
            tool_outputs
                .iter()
                .enumerate()
                .map(|(i, o)| format!("--- tool output {} ---\n{o}", i + 1))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let prompt = format!(
            "<question>\n{question}\n</question>\n\n<tool_outputs>\n{outputs}\n</tool_outputs>\n\n<answer>\n{answer}\n</answer>"
        );

        let body = json!({
            "model": self.model,
            "max_tokens": 4000,
            "system": JUDGE_SYSTEM,
            "messages": [{"role": "user", "content": prompt}],
            "output_config": {
                "format": {
                    "type": "json_schema",
                    "schema": {
                        "type": "object",
                        "properties": {
                            "reasoning": {"type": "string"},
                            "grounded": {"type": "boolean"},
                            "unsupported_claims": {"type": "array", "items": {"type": "string"}},
                            "declined": {"type": "boolean"}
                        },
                        "required": ["reasoning", "grounded", "unsupported_claims", "declined"],
                        "additionalProperties": false
                    }
                }
            }
        });

        let response = self
            .http
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&body)
            .send()
            .await
            .context("judge request failed")?;
        let status = response.status();
        let payload: Value = response.json().await.context("decoding judge response")?;
        if !status.is_success() {
            bail!("judge API error {status}: {payload}");
        }

        let count = |key: &str| payload["usage"][key].as_u64().unwrap_or(0);
        let usage = Usage {
            input_tokens: count("input_tokens"),
            cache_read_input_tokens: count("cache_read_input_tokens"),
            cache_creation_input_tokens: count("cache_creation_input_tokens"),
            output_tokens: count("output_tokens"),
            api_calls: 1,
            elapsed_secs: 0.0,
        };

        if payload["stop_reason"] == "refusal" {
            bail!("judge refused to grade this case");
        }
        let text = payload["content"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|b| b["type"] == "text")
            .and_then(|b| b["text"].as_str())
            .context("judge response has no text block")?;
        let verdict: Verdict =
            serde_json::from_str(text).with_context(|| format!("judge returned invalid JSON: {text}"))?;
        Ok((verdict, usage))
    }
}
