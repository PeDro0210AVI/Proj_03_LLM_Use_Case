use async_std::{fs::File, io::ReadExt};
use serde_json::Value;

use crate::server::primitives::{ContentText, ContentType, McpPayload, McpServerError, ResultType};

pub async fn get_anomalies(threshold: f64) -> Result<McpPayload, McpServerError> {
    let sales = read_json("data/dummy/sales.json").await?;
    let finance = read_json("data/dummy/finance.json").await?;
    let customers = read_json("data/dummy/customers.json").await?;
    let product = read_json("data/dummy/product_health.json").await?;
    let projects = read_json("data/dummy/projects.json").await?;

    let mut anomalies: Vec<String> = Vec::new();

    // Sales: month-over-month revenue change
    let rev_curr = sales["metrics"]["month"]["current"]["revenue"]
        .as_f64()
        .unwrap_or(0.0);
    let rev_prev = sales["metrics"]["month"]["previous"]["revenue"]
        .as_f64()
        .unwrap_or(1.0);
    let rev_change = ((rev_curr - rev_prev) / rev_prev) * 100.0;
    if rev_change.abs() >= threshold {
        let dir = if rev_change > 0.0 { "UP" } else { "DOWN" };
        anomalies.push(format!(
            "[SALES] Monthly revenue {} {:.1}% (${:.2} → ${:.2})",
            dir,
            rev_change.abs(),
            rev_prev,
            rev_curr,
        ));
    }

    // Finance: budget variance
    let variance_pct = finance["budget_vs_actual"]["variance_pct"]
        .as_f64()
        .unwrap_or(0.0);
    if variance_pct.abs() >= threshold {
        anomalies.push(format!(
            "[FINANCE] Budget variance {:.2}% (actual vs budget)",
            variance_pct,
        ));
    }

    // Customers: churn rate vs previous
    let churn_curr = customers["churn_rate"]["monthly_pct"]
        .as_f64()
        .unwrap_or(0.0);
    let churn_prev = customers["churn_rate"]["prev_monthly_pct"]
        .as_f64()
        .unwrap_or(1.0);
    let churn_change = ((churn_curr - churn_prev) / churn_prev) * 100.0;
    if churn_change.abs() >= threshold {
        let dir = if churn_change > 0.0 { "UP" } else { "DOWN" };
        anomalies.push(format!(
            "[CUSTOMERS] Churn rate {} {:.1}% ({:.2}% → {:.2}%)",
            dir,
            churn_change.abs(),
            churn_prev,
            churn_curr,
        ));
    }

    // Product: critical tickets
    let critical = product["open_tickets"]["critical"]
        .as_u64()
        .unwrap_or(0);
    if critical > 0 {
        anomalies.push(format!(
            "[PRODUCT] {} critical ticket(s) open — immediate attention required",
            critical,
        ));
    }

    // Product: uptime below 99.9%
    let uptime = product["uptime"]["current_month_pct"]
        .as_f64()
        .unwrap_or(100.0);
    if uptime < 99.9 {
        anomalies.push(format!(
            "[PRODUCT] Uptime {:.2}% is below 99.9% SLA threshold",
            uptime,
        ));
    }

    // Projects: blocked
    if let Some(project_list) = projects["projects"].as_array() {
        for p in project_list {
            if p["status"].as_str() == Some("blocked") {
                anomalies.push(format!(
                    "[PROJECTS] '{}' ({}) is BLOCKED at {}% completion — deadline: {}",
                    p["name"].as_str().unwrap_or(""),
                    p["department"].as_str().unwrap_or(""),
                    p["completion_pct"].as_u64().unwrap_or(0),
                    p["deadline"].as_str().unwrap_or(""),
                ));
            }
        }
    }

    let text = if anomalies.is_empty() {
        format!(
            "No anomalies detected above {:.0}% threshold. All metrics within normal range.",
            threshold
        )
    } else {
        format!(
            "Anomalies detected (threshold: {:.0}%):\n\n{}",
            threshold,
            anomalies.join("\n")
        )
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
