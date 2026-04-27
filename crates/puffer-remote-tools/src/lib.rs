mod client;
mod model;

pub use client::{describe_capabilities_blocking, execute_tool_blocking};
pub use model::{
    proto, RemoteToolCapabilities, RemoteToolChunk, RemoteToolChunkStream, RemoteToolRequest,
};
