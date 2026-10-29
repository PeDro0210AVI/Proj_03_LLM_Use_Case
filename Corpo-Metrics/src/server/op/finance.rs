use async_std::{fs::File, io::ReadExt};
use serde_json::Value;

use crate::server::primitives::{ContentText, ContentType, McpPayload, McpServerError, ResultType};

pub async fn get_financial_status(category: &str) -> Result<McpPayload, McpServerError> {
    let data = read_json("data/dummy/finance.json").await?;

    let section = &data[category];
    if section.is_null() {
        return Err(McpServerError::CouldntGetCallArguments);
    }

    let text = match category {
        "cash_flow" => format!(
            "Cash Flow:\n  Inflows:  ${:.2}\n  Outflows: ${:.2}\n  Net:      ${:.2}\n  Runway:   {} months",
            section["inflows"].as_f64().unwrap_or(0.0),
            section["outflows"].as_f64().unwrap_or(0.0),
            section["net"].as_f64().unwrap_or(0.0),
            section["runway_months"].as_u64().unwrap_or(0),
        ),
        "expenses" => {
            let b = &section["breakdown"];
            format!(
                "Expenses:\n  Total:          ${:.2}\n  Salaries:       ${:.2}\n  Infrastructure: ${:.2}\n  Marketing:      ${:.2}\n  Operations:     ${:.2}\n  Other:          ${:.2}",
                section["total"].as_f64().unwrap_or(0.0),
                b["salaries"].as_f64().unwrap_or(0.0),
                b["infrastructure"].as_f64().unwrap_or(0.0),
                b["marketing"].as_f64().unwrap_or(0.0),
                b["operations"].as_f64().unwrap_or(0.0),
                b["other"].as_f64().unwrap_or(0.0),
            )
        }
        "profit_margin" => format!(
            "Profit Margins:\n  Gross Margin:  {:.1}%\n  Net Margin:    {:.1}%\n  EBITDA Margin: {:.1}%",
            section["gross_margin_pct"].as_f64().unwrap_or(0.0),
            section["net_margin_pct"].as_f64().unwrap_or(0.0),
            section["ebitda_margin_pct"].as_f64().unwrap_or(0.0),
        ),
        "budget_vs_actual" => format!(
            "Budget vs Actual:\n  Budget:   ${:.2}\n  Actual:   ${:.2}\n  Variance: ${:.2} ({:.2}% under budget)",
            section["budget"].as_f64().unwrap_or(0.0),
            section["actual"].as_f64().unwrap_or(0.0),
            section["variance"].as_f64().unwrap_or(0.0),
            section["variance_pct"].as_f64().unwrap_or(0.0),
        ),
        _ => return Err(McpServerError::CouldntGetCallArguments),
    };

    Ok(McpPayload {
        resultType: ResultType::complete,
        content: vec![ContentText {
            r#type: ContentType::text,
            text,
        }],
    })
}

async fn read_json(path: &str) -> Result<Value, McpServerError> {
    let mut file = File::open(path)
        .await
        .map_err(|_| McpServerError::CouldntFullFilledResponse)?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .await
        .map_err(|_| McpServerError::CouldntFullFilledResponse)?;
    serde_json::from_str(&content).map_err(|_| McpServerError::CouldntFullFilledResponse)
}
