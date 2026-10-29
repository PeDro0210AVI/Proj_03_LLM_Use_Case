use async_std::{fs::File, io::ReadExt};
use serde_json::Value;

use crate::server::primitives::{ContentText, ContentType, McpPayload, McpServerError, ResultType};

pub async fn get_project_status(department: Option<&str>) -> Result<McpPayload, McpServerError> {
    let data = read_json("data/dummy/projects.json").await?;

    let projects = data["projects"]
        .as_array()
        .ok_or(McpServerError::CouldntFullFilledResponse)?;

    let filtered: Vec<&Value> = projects
        .iter()
        .filter(|p| {
            department
                .map(|d| p["department"].as_str().unwrap_or("") == d)
                .unwrap_or(true)
        })
        .collect();

    if filtered.is_empty() {
        return Err(McpServerError::CouldntGetCallArguments);
    }

    let mut text = match department {
        Some(d) => format!("Projects — {}:\n", d),
        None => "All Projects:\n".to_string(),
    };

    for p in &filtered {
        text.push_str(&format!(
            "  [{}] {} | {} | {}% done | Due: {}\n",
            p["priority"].as_str().unwrap_or(""),
            p["name"].as_str().unwrap_or(""),
            p["status"].as_str().unwrap_or(""),
            p["completion_pct"].as_u64().unwrap_or(0),
            p["deadline"].as_str().unwrap_or(""),
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
