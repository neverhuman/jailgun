mod browser;
pub mod bus;
mod mcp;
mod routes;
mod runs;
mod state;
mod ws;

pub use bus::{BroadcastBus, EventBus, NoopBus, RecordingBus};
pub use routes::{api_router, router_with_static, serve};
pub use state::{AppState, BrowserAuthSession, JailgunAgentRunAcceptedResponse};

#[cfg(test)]
mod tests;
