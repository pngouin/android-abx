//! Encoding: the reverse of `decode`. [`writer`] holds [`AbxWriter`], the
//! low-level `Event`→wire-bytes encoder.

mod writer;
pub use writer::AbxWriter;

#[cfg(feature = "xml")]
mod xml;
#[cfg(feature = "xml")]
pub use xml::xml_to_abx;

use crate::{Event, Result};

/// Encode a full `Event` stream to an in-memory ABX byte buffer.
pub fn events_to_abx(events: &[Event]) -> Result<Vec<u8>> {
    let mut w = AbxWriter::new(Vec::new())?;
    for ev in events {
        w.write_event(ev)?;
    }
    Ok(w.into_inner())
}
