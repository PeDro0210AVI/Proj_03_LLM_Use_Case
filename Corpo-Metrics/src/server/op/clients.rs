use async_std::{fs::File, io::ReadExt};
use serde_json::Value;

use crate::server::primitives::{ContentText, ContentType, McpPayload, McpServerError, ResultType};

pub async fn get_customer_metrics(metric: &str) -> Result<McpPayload, McpServerError> {
    let data = read_json("data/dummy/customers.json").await?;

    let section = &data[metric];
    if section.is_null() {
        return Err(McpServerError::CouldntGetCallArguments);
    }

    let text = match metric {
        "active_customers" => format!(
            "Active Customers:\n  Total:              {}\n  Enterprise:         {}\n  Mid-Market:         {}\n  SMB:                {}\n  New This Month:     {}\n  Churned This Month: {}",
            section["total"].as_u64().unwrap_or(0),
            section["enterprise"].as_u64().unwrap_or(0),
            section["mid_market"].as_u64().unwrap_or(0),
            section["smb"].as_u64().unwrap_or(0),
            section["new_this_month"].as_u64().unwrap_or(0),
            section["churned_this_month"].as_u64().unwrap_or(0),
        ),
        "churn_rate" => format!(
            "Churn Rate:\n  Monthly:        {:.2}%\n  Annual:         {:.1}%\n  Prev Monthly:   {:.2}%\n  At-Risk Accts:  {}",
            section["monthly_pct"].as_f64().unwrap_or(0.0),
            section["annual_pct"].as_f64().unwrap_or(0.0),
            section["prev_monthly_pct"].as_f64().unwrap_or(0.0),
            section["at_risk_accounts"].as_u64().unwrap_or(0),
        ),
        "sales_pipeline" => {
            let d = &section["deals"];
            format!(
                "Sales Pipeline:\n  Total Value:        ${:.2}\n  Avg Deal Size:      ${:.2}\n  Prospecting:        {}\n  Qualification:      {}\n  Proposal:           {}\n  Negotiation:        {}\n  Closed This Month:  {}",
                section["total_value"].as_f64().unwrap_or(0.0),
                section["avg_deal_size"].as_f64().unwrap_or(0.0),
                d["prospecting"].as_u64().unwrap_or(0),
                d["qualification"].as_u64().unwrap_or(0),
                d["proposal"].as_u64().unwrap_or(0),
                d["negotiation"].as_u64().unwrap_or(0),
                d["closed_this_month"].as_u64().unwrap_or(0),
            )
        }
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
