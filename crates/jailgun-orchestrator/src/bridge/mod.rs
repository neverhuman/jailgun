pub mod protocol;
pub mod reader;
pub mod spawn;
pub mod writer;

pub use protocol::{
    decode_envelope, encode_envelope, envelope_for_command, envelope_for_event,
    ApproveOrDenyPayload, ArchiveUploadedPayload, BridgeCommand, BridgeEvent, BridgeLogPayload,
    BridgeReadyPayload, BridgeShuttingDownPayload, CloseTabPayload, DownloadCompletePayload,
    DownloadStartedPayload, Envelope, ErrorPayload, GenerationStoppedPayload, HelloPayload,
    MonitorTabPayload, OpenTabPayload, PromptPolicyAppliedPayload, PromptSubmittedPayload,
    ProtocolError, ShutdownPayload, SubmitPromptPayload, TabClosedPayload, TabOpenedPayload,
    TabProgressKind, TabProgressPayload, TarDiscoveredPayload, ToolPromptDetectedPayload,
    UploadArchivePayload, MAX_LINE_BYTES, PROTOCOL_VERSION,
};
pub use spawn::{spawn_bridge, BridgeHandle, BridgeSpawnConfig};
