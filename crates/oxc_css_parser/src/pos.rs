#[cfg(feature = "serialize")]
use serde::Serialize;

/// Span represents a range of a piece of source code.
/// It counts by offset, so it's 0-based.
///
/// Offsets are `u32` (matching oxc convention), so a larger source fails to parse (`ErrorKind::SourceTooLong`).
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub struct Span {
    /// Start offset. (Inclusive)
    pub start: u32,
    /// End offset. (Exclusive)
    pub end: u32,
}

impl Span {
    /// The source text this span covers.
    pub fn slice(self, source: &str) -> &str {
        &source[self.start as usize..self.end as usize]
    }
}
