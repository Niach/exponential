//! EXP-877 — the ONE work header shared by the issue face, the run face and
//! the diff face of a top tab (issue detail + session screen), byte-identical
//! with the web `WorkHeader`. Lane D-B fills this module in; the column width
//! is pinned first so every face (header, issue body, transcript, diff) shares
//! it.

/// The shared work column width — web `max-w-4xl` (896px): header, issue
/// body, transcript and the full-page diff all cap to it.
pub(crate) const WORK_COLUMN_W: f32 = 896.;
