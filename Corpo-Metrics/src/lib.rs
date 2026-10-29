use std::error::Error;

use crate::{config::Env, http_engine::HttpServer};

mod config;
pub mod http_engine;
pub mod server;

pub struct CorpoMCP;

impl CorpoMCP {
    pub async fn init() -> Result<(), Box<dyn Error>> {
        let config = Env::env_init();

        let server = HttpServer::new(&config.host, config.port).await;

        println!("Started server");

        Ok(server.run().await?)
    }
}
