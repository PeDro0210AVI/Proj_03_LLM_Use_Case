/// USD per million tokens (input, output) for the models this app is likely
/// to be pointed at. Anthropic first-party list prices; cache reads bill at
/// 0.1x and cache writes at 1.25x the input price.
fn price_per_mtok(model: &str) -> Option<(f64, f64)> {
    let price = match model {
        "claude-fable-5-1" | "claude-fable-5" => (10.0, 50.0),
        "claude-opus-5-5" => (4.0, 20.0),
        "claude-opus-5" | "claude-opus-4-8" | "claude-opus-4-7" | "claude-opus-4-6" => (5.0, 25.0),
        "claude-sonnet-5-5" | "claude-sonnet-5" => (2.0, 10.0),
        "claude-sonnet-4-6" => (3.0, 15.0),
        "claude-haiku-5-5" => (0.10, 0.50),
        "claude-haiku-4-5" => (1.0, 5.0),
        _ => return None,
    };
    Some(price)
}

/// Token usage for one user turn, summed over every model call it made.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// Uncached input tokens.
    pub input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub output_tokens: u64,
    /// Number of `POST /v1/messages` calls (1 + one per tool round trip).
    pub api_calls: u32,
    pub elapsed_secs: f64,
}

impl Usage {
    pub fn total_input_tokens(&self) -> u64 {
        self.input_tokens + self.cache_read_input_tokens + self.cache_creation_input_tokens
    }

    /// Estimated cost in USD, or `None` for a model without a known price.
    pub fn cost_usd(&self, model: &str) -> Option<f64> {
        let (input, output) = price_per_mtok(model)?;
        let input_cost = self.input_tokens as f64 * input
            + self.cache_read_input_tokens as f64 * input * 0.1
            + self.cache_creation_input_tokens as f64 * input * 1.25;
        Some((input_cost + self.output_tokens as f64 * output) / 1_000_000.0)
    }

    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
        self.cache_creation_input_tokens += other.cache_creation_input_tokens;
        self.output_tokens += other.output_tokens;
        self.api_calls += other.api_calls;
        self.elapsed_secs += other.elapsed_secs;
    }
}
