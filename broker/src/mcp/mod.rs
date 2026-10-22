pub mod bytecode_parser;
pub mod isolation;
pub mod streaming;
pub mod tools;

use std::sync::Arc;

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    model::{ServerCapabilities, ServerConfig},
    tool_handler,
};

use crate::Context;

#[derive(Clone)]
pub struct RoprobeMcp {
    pub ctx: Context,
    pub agent_id: String,
    pub locks: Arc<isolation::LockTable>,
    pub streams: Arc<streaming::Streams>,
    tool_router: ToolRouter<Self>,
}

impl RoprobeMcp {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            agent_id: format!("{:016x}", rand::random::<u64>()),
            locks: Arc::default(),
            streams: Arc::default(),
            tool_router: tools::router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RoprobeMcp {
    #[allow(deprecated)] // logging carries the streams
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_logging().build())
    }
}

pub async fn start(ctx: Context) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mcp = RoprobeMcp::new(ctx);
    let (locks, me) = (mcp.locks.clone(), mcp.agent_id.clone());
    let svc = mcp.serve(rmcp::transport::stdio()).await?;
    tracing::info!("mcp up on stdio as {me}");
    let res = svc.waiting().await;
    locks.clear_agent(&me).await;
    res?;
    Ok(())
}
