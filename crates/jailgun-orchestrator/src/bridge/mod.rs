pub mod command;
pub mod event;
pub mod protocol;
pub mod reader;
pub mod spawn;
pub mod writer;

pub use command::{
    ApproveOrDenyPayload, BridgeCommand, CloseTabPayload, HelloPayload, MonitorTabPayload,
    OpenTabPayload, ShutdownPayload, SubmitPromptPayload, UploadArchivePayload,
};
pub use event::{
    ArchiveUploadedPayload, BridgeEvent, BridgeLogPayload, BridgeReadyPayload,
    BridgeShuttingDownPayload, DownloadCompletePayload, DownloadStartedPayload, ErrorPayload,
    GenerationStoppedPayload, PromptPolicyAppliedPayload, PromptSubmittedPayload,
    RateLimitDetectedPayload, TabClosedPayload, TabOpenedPayload, TabProgressKind,
    TabProgressPayload, TarDiscoveredPayload, ToolPromptDetectedPayload,
};
pub use protocol::{
    decode_envelope, encode_envelope, envelope_for_command, envelope_for_event, Envelope,
    ProtocolError, MAX_LINE_BYTES, PROTOCOL_VERSION,
};
pub use spawn::{spawn_bridge, BridgeHandle, BridgeSpawnConfig};
