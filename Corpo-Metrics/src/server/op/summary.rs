use async_std::{fs::File, io::ReadExt};
use serde_json::Value;

use crate::server::primitives::{ContentText, ContentType, McpPayload, McpServerError, ResultType};

pub async fn generate_executive_summary(
    departments: Option<&[String]>,
) -> Result<McpPayload, McpServerError> {
    let include_all = departments.map(|d| d.is_empty()).unwrap_or(true);

    let wants = |name: &str| -> bool {
        include_all
            || departments
                .unwrap_or(&[])
                .iter()
                .any(|d| d.eq_ignore_ascii_case(name))
    };

    let mut sections: Vec<String> = Vec::new();
    sections.push("=== Executive Summary ===".to_string());

    if wants("Ventas") || wants("Sales") {
        let data = read_json("data/dummy/sales.json").await?;
        let m = &data["metrics"]["month"]["current"];
        let prev = &data["metrics"]["month"]["previous"];
        let rev = m["revenue"].as_f64().unwrap_or(0.0);
        let prev_rev = prev["revenue"].as_f64().unwrap_or(1.0);
        let change = ((rev - prev_rev) / prev_rev) * 100.0;
        let sign = if change >= 0.0 { "+" } else { "" };
        sections.push(format!(
            "\n[Sales]\n  Monthly Revenue: ${:.2}  ({}{:.1}% MoM)\n  Orders:          {}\n  Units Sold:      {}",
            rev, sign, change,
            m["orders"].as_u64().unwrap_or(0),
            m["units_sold"].as_u64().unwrap_or(0),
        ));
    }

    if wants("Finanzas") || wants("Finance") {
        let data = read_json("data/dummy/finance.json").await?;
        sections.push(format!(
            "\n[Finance]\n  Net Cash Flow:   ${:.2}\n  Runway:          {} months\n  Gross Margin:    {:.1}%\n  Net Margin:      {:.1}%",
            data["cash_flow"]["net"].as_f64().unwrap_or(0.0),
            data["cash_flow"]["runway_months"].as_u64().unwrap_or(0),
            data["profit_margin"]["gross_margin_pct"].as_f64().unwrap_or(0.0),
            data["profit_margin"]["net_margin_pct"].as_f64().unwrap_or(0.0),
        ));
    }

    if wants("Clientes") || wants("Customers") {
        let data = read_json("data/dummy/customers.json").await?;
        sections.push(format!(
            "\n[Customers]\n  Active:          {}\n  Monthly Churn:   {:.2}%\n  Pipeline Value:  ${:.2}",
            data["active_customers"]["total"].as_u64().unwrap_or(0),
            data["churn_rate"]["monthly_pct"].as_f64().unwrap_or(0.0),
            data["sales_pipeline"]["total_value"].as_f64().unwrap_or(0.0),
        ));
    }

    if wants("Equipo") || wants("HR") || wants("Teams") {
        let data = read_json("data/dummy/teams.json").await?;
        sections.push(format!(
            "\n[Team]\n  Headcount:       {}\n  Open Vacancies:  {}",
            data["headcount"]["total"].as_u64().unwrap_or(0),
            data["vacancies"]["total"].as_u64().unwrap_or(0),
        ));
    }

    if wants("Producto") || wants("Product") {
        let data = read_json("data/dummy/product_health.json").await?;
        sections.push(format!(
            "\n[Product]\n  MAU:             {}\n  Uptime:          {:.2}%\n  Critical Tickets:{} ",
            data["active_users"]["mau"].as_u64().unwrap_or(0),
            data["uptime"]["current_month_pct"].as_f64().unwrap_or(0.0),
            data["open_tickets"]["critical"].as_u64().unwrap_or(0),
        ));
    }

    if wants("Proyectos") || wants("Projects") {
        let data = read_json("data/dummy/projects.json").await?;
        let projects = data["projects"].as_array().unwrap_or(&vec![]).clone();
        let blocked = projects
            .iter()
            .filter(|p| p["status"].as_str() == Some("blocked"))
            .count();
        let in_progress = projects
            .iter()
            .filter(|p| p["status"].as_str() == Some("in_progress"))
            .count();
        sections.push(format!(
            "\n[Projects]\n  In Progress:     {}\n  Blocked:         {}",
            in_progress, blocked,
        ));
    }

    Ok(McpPayload {
        resultType: ResultType::complete,
        content: vec![ContentText {
            r#type: ContentType::text,
            text: sections.join(""),
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
