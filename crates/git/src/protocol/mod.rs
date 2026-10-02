//! Klotho's own implementation of the server side of the git protocol, built
//! on gitoxide (ADR 0004). gitoxide reads and writes repositories, walks
//! history and builds and indexes packs; this module speaks the wire protocol
//! around that, which gitoxide only implements for clients.
//!
//! It has no HTTP or SSH in it: callers hand it a repository and the request
//! bytes, and it writes the response.
//!
//! - Fetch and clone: protocol v2 ([`serve_v2`]) and v0/v1 ([`serve_upload_pack`]),
//!   including shallow fetches. Partial clone filters aren't supported.
//! - Push: [`serve_receive_pack`] (git has no v2 push).

mod fetch;
mod pack;
pub mod pktline;
mod receive;
mod refs;
mod select;
mod sideband;
mod v0;
mod v2;

use std::io::Write;

pub use receive::{ReceiveHooks, RefUpdate, serve_receive_pack, write_receive_pack_advertisement};
pub use v0::{serve_upload_pack, write_upload_pack_advertisement};
pub use v2::{serve as serve_v2, write_advertisement as write_v2_advertisement};

/// Most `want`, `have` or ref update lines a single request may carry. Git sends
/// haves in rounds of at most a few hundred, and wants are bounded by the
/// number of refs.
const MAX_LINES: usize = 100_000;

/// Something wrong with what the client sent. Its message goes back to the
/// client, so it must never contain server details.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ProtocolError(String);

impl ProtocolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// Writing the response failed, usually because the client went away.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Git(#[from] crate::Error),
}

/// Answers with an `ERR` line, which the client prints as "remote error".
fn answer_error(out: &mut dyn Write, err: ProtocolError) -> Result<(), ServeError> {
    pktline::write_line(out, &format!("ERR {err}"))?;
    pktline::write_flush(out)?;
    Ok(())
}
