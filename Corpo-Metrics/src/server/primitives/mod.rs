use std::{error::Error, fmt::Display};

use async_std::{fs::File, io::ReadExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::server::op::{
    generate_executive_summary, get_anomalies, get_customer_metrics, get_financial_status,
    get_inventory_levels, get_product_health, get_project_status, get_sales_metrics,
    get_team_metrics, get_top_products,
};

const MCP_INITIALIZE_RAW: &str = "initialize";
const MCP_TOOLS_LIST_RAW: &str = "tools/list";
const MCP_TOOLS_CALL_RAW: &str = "tools/call";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

// stateless
pub struct McpServer;

// JsonRPC response
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct McpJson {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<MCPCallparams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpJsonError>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpJsonError {
    code: i32,
    message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MCPCallparams {
    name: Option<String>,
    arguments: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ResultType {
    complete,
    inputRequired,
    task,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ContentType {
    text,
    // just adding text for the moment
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContentText {
    pub r#type: ContentType,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpToolListNames {
    names: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpPayload {
    pub resultType: ResultType,
    pub content: Vec<ContentText>,
}

#[derive(Debug)]
pub enum McpServerError {
    NotMethodSupply,
    CouldntFullFilledResponse,
    CouldntGetCallArguments,
}

impl Display for McpServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpServerError::NotMethodSupply => write!(f, "no method provided in request"),
            McpServerError::CouldntFullFilledResponse => write!(f, "unsupported method"),
            McpServerError::CouldntGetCallArguments => write!(f, "unknown tool name"),
        }
    }
}

impl Error for McpServerError {}

// wrapper
impl McpServer {
    // returns None for JSON-RPC notifications (no "id"), since the spec
    // forbids sending a response to those; Some(McpJson) otherwise
    // (success or JSON-RPC error), which the caller writes back to the client
    pub async fn handle_response(payload: Value) -> Option<McpJson> {
        let is_notification = payload.get("id").is_none();
        let id = payload.get("id").and_then(Value::as_u64).map(|v| v as u32);

        let json_rpc_request = match serde_json::from_value::<McpJson>(payload) {
            Ok(req) => req,
            Err(err) => {
                return (!is_notification)
                    .then(|| McpServer::error_response(id, -32600, err.to_string()));
            }
        };

        let method = match json_rpc_request.method.as_deref() {
            Some(method) => method,
            None => {
                return (!is_notification).then(|| {
                    McpServer::error_response(id, -32600, "method not provided".to_string())
                });
            }
        };

        let result: Result<McpJson, Box<dyn Error>> = match method {
            MCP_INITIALIZE_RAW => McpServer::mcp_initialize().await,
            MCP_TOOLS_CALL_RAW => McpServer::mcp_call(&json_rpc_request).await,
            MCP_TOOLS_LIST_RAW => McpServer::mcp_list().await,
            _ => Err(Box::new(McpServerError::CouldntFullFilledResponse)),
        };

        if is_notification {
            return None;
        }

        Some(match result {
            Ok(mut response) => {
                response.id = id;
                response
            }
            Err(err) => McpServer::error_response(id, McpServer::error_code(&err), err.to_string()),
        })
    }

    fn error_code(err: &Box<dyn Error>) -> i32 {
        match err.downcast_ref::<McpServerError>() {
            Some(McpServerError::NotMethodSupply) => -32600,
            Some(McpServerError::CouldntFullFilledResponse) => -32601,
            Some(McpServerError::CouldntGetCallArguments) => -32602,
            None => -32603,
        }
    }

    pub fn error_response(id: Option<u32>, code: i32, message: String) -> McpJson {
        McpJson {
            jsonrpc: "2.0".to_string(),
            id,
            error: Some(McpJsonError { code, message }),
            ..Default::default()
        }
    }

    async fn mcp_initialize() -> Result<McpJson, Box<dyn Error>> {
        let result = serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": env!("CARGO_PKG_NAME"),
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        Ok(McpJson {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            ..Default::default()
        })
    }

    //TODO: all opening files things load them at startup
    async fn mcp_call(json_rpc_request: &McpJson) -> Result<McpJson, Box<dyn Error>> {
        if let None = json_rpc_request.params {
            return Err(Box::new(McpServerError::CouldntFullFilledResponse));
        };

        let mcp_params = json_rpc_request.params.as_ref().unwrap();

        println!("param send: {}", mcp_params.name.as_ref().unwrap());

        let args = mcp_params.arguments.as_ref();

        let result: Result<McpPayload, McpServerError> =
            match mcp_params.name.as_ref().unwrap().as_str() {
                "get_sales_metrics" => {
                    let period = args
                        .and_then(|a| a["period"].as_str())
                        .ok_or(McpServerError::CouldntGetCallArguments)?;
                    let compare = args
                        .and_then(|a| a["compare_to_previous"].as_bool())
                        .unwrap_or(false);
                    get_sales_metrics(period, compare).await
                }
                "get_top_products" => {
                    let limit = args
                        .and_then(|a| a["limit"].as_u64())
                        .unwrap_or(5) as usize;
                    get_top_products(limit).await
                }
                "get_financial_status" => {
                    let category = args
                        .and_then(|a| a["category"].as_str())
                        .ok_or(McpServerError::CouldntGetCallArguments)?;
                    get_financial_status(category).await
                }
                "get_customer_metrics" => {
                    let metric = args
                        .and_then(|a| a["metric"].as_str())
                        .ok_or(McpServerError::CouldntGetCallArguments)?;
                    get_customer_metrics(metric).await
                }
                "get_project_status" => {
                    let department = args.and_then(|a| a["department"].as_str());
                    get_project_status(department).await
                }
                "get_team_metrics" => {
                    let metric = args
                        .and_then(|a| a["metric"].as_str())
                        .ok_or(McpServerError::CouldntGetCallArguments)?;
                    get_team_metrics(metric).await
                }
                "get_product_health" => {
                    let metric = args
                        .and_then(|a| a["metric"].as_str())
                        .ok_or(McpServerError::CouldntGetCallArguments)?;
                    get_product_health(metric).await
                }
                "generate_executive_summary" => {
                    let departments: Option<Vec<String>> = args.and_then(|a| {
                        a["departments"].as_array().map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                    });
                    generate_executive_summary(departments.as_deref()).await
                }
                "get_anomalies" => {
                    let threshold = args
                        .and_then(|a| a["threshold"].as_f64())
                        .unwrap_or(20.0);
                    get_anomalies(threshold).await
                }
                "get_inventory_levels" => {
                    let category = args.and_then(|a| a["category"].as_str());
                    get_inventory_levels(category).await
                }
                _ => Err(McpServerError::CouldntGetCallArguments),
            };

        let result = Some(serde_json::to_value(result?)?);

        Ok(McpJson {
            jsonrpc: "2.0".to_string(),
            result,
            ..Default::default()
        })
    }

    async fn mcp_list() -> Result<McpJson, Box<dyn Error>> {
        // grabbing the tools_list json
        let mut file = File::open("data/mcp/tools_list.json").await?;

        let content = &mut String::new();

        file.read_to_string(content).await?;

        let mut deserializer = serde_json::Deserializer::from_str(&content);
        let deserializer = serde_stacker::Deserializer::new(&mut deserializer);
        let value = Value::deserialize(deserializer)?;

        Ok(McpJson {
            jsonrpc: "2.0".to_string(),
            result: Some(value),
            ..Default::default()
        })
    }
}
