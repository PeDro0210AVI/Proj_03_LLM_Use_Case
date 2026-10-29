use std::error::Error;

use corpo_metrics::CorpoMCP;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    CorpoMCP::init().await
}
