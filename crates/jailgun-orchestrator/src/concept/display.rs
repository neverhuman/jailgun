//! Per-account Linux display and private RFB transport. No network listeners.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{LoginView, ManagedDisplay};

#[cfg(not(target_os = "linux"))]
mod unsupported {
    use std::{collections::BTreeMap, path::Path};
    pub struct ManagedDisplay;
    pub struct LoginView {
        pub stream: tokio::net::UnixStream,
    }
    impl ManagedDisplay {
        pub async fn start(_: &Path) -> anyhow::Result<Self> {
            anyhow::bail!("server-browser-unsupported: use the local Chrome window on macOS")
        }
        pub fn environment(&self) -> BTreeMap<String, String> {
            BTreeMap::new()
        }
        pub async fn alive(&self) -> bool {
            false
        }
        pub async fn open_view(&self) -> anyhow::Result<LoginView> {
            anyhow::bail!("login-view-unavailable")
        }
        pub async fn shutdown(&self) {}
    }
    impl LoginView {
        pub async fn shutdown(&mut self) {}
    }
}
#[cfg(not(target_os = "linux"))]
pub use unsupported::{LoginView, ManagedDisplay};
