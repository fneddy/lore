//! Main entry point for rmcp-lore MCP server

use lore_cli::mcp::LoreHandler;
use rmcp::service::ServiceExt;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    env_logger::init();

    log::info!("Starting rmcp-lore MCP server");

    let handler = LoreHandler::new();
    log::info!(
        "registered tools: {:?}",
        handler
            .get_tool_router()
            .list_all()
            .iter()
            .map(|t| t.name.as_ref())
            .collect::<Vec<_>>()
    );

    // Serve the MCP server over rmcp's stdio transport helper and keep the
    // running service alive until the connection is closed.
    let server = handler
        .serve(rmcp::transport::stdio())
        .await
        .expect("Failed to start MCP server");

    log::info!("rmcp-lore MCP server started; waiting for shutdown");

    server
        .waiting()
        .await
        .expect("MCP server terminated unexpectedly");

    log::info!("rmcp-lore MCP server stopped");
}
