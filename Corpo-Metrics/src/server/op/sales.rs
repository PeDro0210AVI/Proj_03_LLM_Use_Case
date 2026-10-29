use async_std::{fs::File, io::ReadExt};
use serde_json::Value;

use crate::server::primitives::{ContentText, ContentType, McpPayload, McpServerError, ResultType};

pub async fn get_sales_metrics(
    period: &str,
    compare_to_previous: bool,
) -> Result<McpPayload, McpServerError> {
    let data = read_json("data/dummy/sales.json").await?;

    let current = &data["metrics"][period]["current"];
    if current.is_null() {
        return Err(McpServerError::CouldntGetCallArguments);
    }

    let mut text = format!(
        "Sales Metrics ({}):\n  Revenue:     ${:.2}\n  Orders:      {}\n  Avg Order:   ${:.2}\n  Units Sold:  {}",
        period,
        current["revenue"].as_f64().unwrap_or(0.0),
        current["orders"].as_u64().unwrap_or(0),
        current["avg_order"].as_f64().unwrap_or(0.0),
        current["units_sold"].as_u64().unwrap_or(0),
    );

    if compare_to_previous {
        let previous = &data["metrics"][period]["previous"];
        let rev_curr = current["revenue"].as_f64().unwrap_or(0.0);
        let rev_prev = previous["revenue"].as_f64().unwrap_or(1.0);
        let change_pct = ((rev_curr - rev_prev) / rev_prev) * 100.0;
        let sign = if change_pct >= 0.0 { "+" } else { "" };
        text.push_str(&format!(
            "\n\nVs Previous {}:\n  Revenue Change: {}{:.1}%\n  Previous Revenue: ${:.2}",
            period, sign, change_pct, rev_prev,
        ));
    }

    Ok(McpPayload {
        resultType: ResultType::complete,
        content: vec![ContentText {
            r#type: ContentType::text,
            text,
        }],
    })
}

pub async fn get_top_products(limit: usize) -> Result<McpPayload, McpServerError> {
    let data = read_json("data/dummy/products.json").await?;

    let products = data["products"]
        .as_array()
        .ok_or(McpServerError::CouldntFullFilledResponse)?;

    let count = limit.min(products.len());
    let mut text = format!("Top {} Products by Revenue:\n", count);

    for product in products.iter().take(count) {
        text.push_str(&format!(
            "  {}. {} | ${:.2} revenue | {} units | {}\n",
            product["rank"].as_u64().unwrap_or(0),
            product["name"].as_str().unwrap_or("Unknown"),
            product["revenue"].as_f64().unwrap_or(0.0),
            product["units_sold"].as_u64().unwrap_or(0),
            product["category"].as_str().unwrap_or(""),
        ));
    }

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
