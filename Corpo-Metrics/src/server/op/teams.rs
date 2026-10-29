use async_std::{fs::File, io::ReadExt};
use serde_json::Value;

use crate::server::primitives::{ContentText, ContentType, McpPayload, McpServerError, ResultType};

pub async fn get_team_metrics(metric: &str) -> Result<McpPayload, McpServerError> {
    let data = read_json("data/dummy/teams.json").await?;

    let section = &data[metric];
    if section.is_null() {
        return Err(McpServerError::CouldntGetCallArguments);
    }

    let text = match metric {
        "headcount" => {
            let by_dept = &section["by_department"];
            format!(
                "Headcount:\n  Total:       {}\n  Full-Time:   {}\n  Contractors: {}\n\nBy Department:\n  Engineering: {}\n  Sales:       {}\n  Marketing:   {}\n  Operations:  {}\n  HR:          {}\n  Finance:     {}\n  Leadership:  {}",
                section["total"].as_u64().unwrap_or(0),
                section["full_time"].as_u64().unwrap_or(0),
                section["contractors"].as_u64().unwrap_or(0),
                by_dept["Engineering"].as_u64().unwrap_or(0),
                by_dept["Sales"].as_u64().unwrap_or(0),
                by_dept["Marketing"].as_u64().unwrap_or(0),
                by_dept["Operations"].as_u64().unwrap_or(0),
                by_dept["HR"].as_u64().unwrap_or(0),
                by_dept["Finance"].as_u64().unwrap_or(0),
                by_dept["Leadership"].as_u64().unwrap_or(0),
            )
        }
        "vacancies" => {
            let by_dept = &section["by_department"];
            format!(
                "Vacancies:\n  Total Open:      {}\n  Offers Pending:  {}\n  Avg Fill Time:   {} days\n\nBy Department:\n  Engineering:     {}\n  Sales:           {}\n  Marketing:       {}\n  Operations:      {}",
                section["total"].as_u64().unwrap_or(0),
                section["offers_pending"].as_u64().unwrap_or(0),
                section["avg_time_to_fill_days"].as_u64().unwrap_or(0),
                by_dept["Engineering"].as_u64().unwrap_or(0),
                by_dept["Sales"].as_u64().unwrap_or(0),
                by_dept["Marketing"].as_u64().unwrap_or(0),
                by_dept["Operations"].as_u64().unwrap_or(0),
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
