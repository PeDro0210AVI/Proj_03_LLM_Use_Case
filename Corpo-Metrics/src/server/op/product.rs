use async_std::{fs::File, io::ReadExt};
use serde_json::Value;

use crate::server::primitives::{ContentText, ContentType, McpPayload, McpServerError, ResultType};

pub async fn get_product_health(metric: &str) -> Result<McpPayload, McpServerError> {
    let data = read_json("data/dummy/product_health.json").await?;

    let section = &data[metric];
    if section.is_null() {
        return Err(McpServerError::CouldntGetCallArguments);
    }

    let text = match metric {
        "active_users" => format!(
            "Active Users:\n  DAU:             {}\n  WAU:             {}\n  MAU:             {}\n  DAU/MAU Ratio:   {:.3}\n  MoM Growth:      {:.1}%",
            section["dau"].as_u64().unwrap_or(0),
            section["wau"].as_u64().unwrap_or(0),
            section["mau"].as_u64().unwrap_or(0),
            section["dau_mau_ratio"].as_f64().unwrap_or(0.0),
            section["growth_mom_pct"].as_f64().unwrap_or(0.0),
        ),
        "uptime" => format!(
            "Uptime:\n  Current Month:   {:.2}%\n  Last Incident:   {}\n  Incidents YTD:   {}\n  Avg Response:    {} ms",
            section["current_month_pct"].as_f64().unwrap_or(0.0),
            section["last_incident"].as_str().unwrap_or("N/A"),
            section["incident_count_ytd"].as_u64().unwrap_or(0),
            section["avg_response_ms"].as_u64().unwrap_or(0),
        ),
        "open_tickets" => format!(
            "Open Tickets:\n  Total:           {}\n  Critical:        {}\n  High:            {}\n  Medium:          {}\n  Low:             {}\n  Avg Resolution:  {:.1} hours",
            section["total"].as_u64().unwrap_or(0),
            section["critical"].as_u64().unwrap_or(0),
            section["high"].as_u64().unwrap_or(0),
            section["medium"].as_u64().unwrap_or(0),
            section["low"].as_u64().unwrap_or(0),
            section["avg_resolution_hours"].as_f64().unwrap_or(0.0),
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
