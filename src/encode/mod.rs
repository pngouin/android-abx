mod writer;
pub use writer::AbxWriter;

#[cfg(feature = "xml")]
mod xml;
#[cfg(feature = "xml")]
pub use xml::xml_to_abx;

use crate::{Event, Result};

/// Encodes a list of events as ABX bytes.
///
/// Shorthand for writing each event with an [`AbxWriter`] over a `Vec<u8>`.
///
/// # Errors
///
/// Returns [`AbxError::ValueTooLong`](crate::AbxError::ValueTooLong) if a string
/// or byte value is longer than 65,535 bytes.
///
/// # Examples
///
/// ```
/// use android_abx::{Event, events_to_abx};
///
/// let data = events_to_abx(&[Event::StartDocument, Event::EndDocument])?;
/// assert!(data.starts_with(&android_abx::MAGIC));
/// # Ok::<(), android_abx::AbxError>(())
/// ```
pub fn events_to_abx(events: &[Event]) -> Result<Vec<u8>> {
    let mut w = AbxWriter::new(Vec::new())?;
    for ev in events {
        w.write_event(ev)?;
    }
    Ok(w.into_inner())
}
