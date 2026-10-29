use async_std::{fs::File, io::ReadExt};
use serde_json::Value;

use crate::server::primitives::{ContentText, ContentType, McpPayload, McpServerError, ResultType};

pub async fn get_inventory_levels(category: Option<&str>) -> Result<McpPayload, McpServerError> {
    let data = read_json("data/dummy/inventory.json").await?;

    let categories = data["categories"]
        .as_object()
        .ok_or(McpServerError::CouldntFullFilledResponse)?;

    let last_updated = data["last_updated"].as_str().unwrap_or("N/A");

    let mut text = format!("Inventory Levels (updated: {}):\n", last_updated);

    let entries: Vec<(&String, &Value)> = match category {
        Some(cat) => {
            let entry = categories
                .get(cat)
                .ok_or(McpServerError::CouldntGetCallArguments)?;
            vec![(
                categories
                    .keys()
                    .find(|k| k.as_str() == cat)
                    .unwrap(),
                entry,
            )]
        }
        None => categories.iter().collect(),
    };

    for (name, section) in entries {
        text.push_str(&format!(
            "\n  {}:\n    Available:     {}\n    In Use:        {}\n    Reserved:      {}\n    Expiring Soon: {}\n",
            name,
            section["available"],
            section["in_use"],
            section["reserved"],
            section["expiring_soon"],
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
