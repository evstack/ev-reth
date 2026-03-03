/// records `duration_ms` on the current tracing span when dropped,
/// ensuring duration is captured even on early-return error paths.
pub(crate) struct RecordDurationOnDrop(std::time::Instant);

impl RecordDurationOnDrop {
    pub(crate) fn new() -> Self {
        Self(std::time::Instant::now())
    }
}

impl Drop for RecordDurationOnDrop {
    fn drop(&mut self) {
        tracing::Span::current().record("duration_ms", self.0.elapsed().as_millis() as u64);
    }
}
