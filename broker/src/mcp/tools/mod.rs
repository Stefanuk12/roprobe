mod automation;
mod bytecode;
mod core;
mod screenshot;
mod volt;

use rmcp::handler::server::router::tool::ToolRouter;

use super::RoprobeMcp;

pub fn router() -> ToolRouter<RoprobeMcp> {
    RoprobeMcp::core_router()
        + RoprobeMcp::bytecode_router()
        + RoprobeMcp::automation_router()
        + RoprobeMcp::screenshot_router()
        + RoprobeMcp::volt_router()
}
