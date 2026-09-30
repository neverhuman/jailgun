//! Lightweight, operator-only agent worker used by the MCP control plane.
//!
//! Each logical tab owns one private workspace and at most one active job. The
//! production executor invokes Codex without a shell; tests inject a fake
//! executor and never require credentials, a browser, or a live model.

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use jailgun_workflow::{Error, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tar::{Archive, Builder};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::{watch, Mutex, RwLock},
};

const MAX_OBJECT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CHUNK_BYTES: usize = 256 * 1024;
const MAX_PROMPT_CHARS: usize = 128_000;
const DEFAULT_TIMEOUT_SECONDS: u64 = 30 * 60;
const MAX_TIMEOUT_SECONDS: u64 = 24 * 60 * 60;
const AUTH_CHECK_TIMEOUT_SECONDS: u64 = 10;

mod archive;
mod executor;
mod jobs;
mod model;
mod objects;
mod service;
mod support;
mod tabs;

pub use executor::ProcessExecutor;
pub use model::*;
use service::UploadState;
pub use service::WorkerService;

use archive::*;
use support::*;

#[cfg(test)]
mod tests;
