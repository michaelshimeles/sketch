//! MCP server: AI agents read and edit the canvas of the running app.
//!
//! Modeled on Paper's MCP server (behaviour notes in `analysis/paper-mcp.md`): the app serves
//! MCP over streamable HTTP on loopback, and `sketch-rs-mcp` bridges stdio clients to it. Tool
//! calls are handed to the thread that owns the editor, so every change goes through the
//! normal transaction / undo path and repaints the window.
//!
//! * [`server`]: the rmcp HTTP service (runs on its own Tokio thread).
//! * [`exec`]: runs one tool call against a [`exec::Host`] (the app, or a headless session).
//! * [`tools`]: tool names, descriptions and schemas; [`css`] and [`html`]: the style and
//!   markup mapping used by `write_html` / `update_styles`.

pub mod css;
pub mod exec;
pub mod html;
pub mod server;
pub mod tools;
pub mod workspaces;

/// Paper uses 29979; Sketch listens next door so both apps can run side by side.
pub const DEFAULT_PORT: u16 = 29_980;
pub const ENDPOINT: &str = "/mcp";

/// The port to serve on / connect to: `$SKETCH_RS_MCP_PORT` (or `$PAPER_RS_MCP_PORT`), else
/// [`DEFAULT_PORT`].
pub fn port() -> u16 {
    crate::env_var("MCP_PORT")
        .and_then(|p| p.to_str()?.trim().parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

pub fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}{ENDPOINT}")
}

/// One piece of tool output.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Text(String),
    Image { data: Vec<u8>, mime: String },
}

pub type ToolResult = Result<Vec<Out>, String>;

/// A tool call travelling from the HTTP thread to the editor thread.
pub struct Call {
    pub name: String,
    pub args: serde_json::Value,
    pub reply: tokio::sync::oneshot::Sender<ToolResult>,
}

/// Run calls arriving on `rx` against a host until every sender is gone. For hosts that
/// own their editor on a plain thread (tests, headless use); the app drains its channel
/// from the event loop instead.
pub fn serve_host(host: &mut dyn exec::Host, rx: std::sync::mpsc::Receiver<Call>) {
    while let Ok(call) = rx.recv() {
        let r = exec::call(host, &call.name, &call.args);
        let _ = call.reply.send(r);
    }
}
