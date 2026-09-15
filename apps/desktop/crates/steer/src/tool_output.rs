//! EXP-895 — the command output a `tool_update` carries at settle.
//!
//! One pure helper, [`truncate_output`], the tail-cut twin of
//! [`crate::tool_diff::truncate_unified_diff`]: a diff is read from the TOP
//! (its headers name the file), a command's output from the BOTTOM (its
//! verdict is the last thing it wrote), so this keeps the LAST lines and the
//! caller marks the cut at the FRONT — the same `\ N more lines truncated`
//! marker line the cut patches wear, where the dropped lines were.
//!
//! Only `execute` calls publish output, only ONCE, at the settle: a live tail
//! streaming to every viewer is the local card's job (the engine sees strictly
//! more than the wire does) and the row a reader scrolls back to wants the
//! verdict, not the stream.

/// The contract's per-call output caps (`steerFeed.toolOutputMaxLines` /
/// `toolOutputMaxBytes`) — what the mapper cuts to, and what the relay's zod
/// admits (plus the one-line truncation marker).
pub const TOOL_OUTPUT_MAX_LINES: usize = domain::contract::STEER_FEED_TOOL_OUTPUT_MAX_LINES;
pub const TOOL_OUTPUT_MAX_BYTES: usize = domain::contract::STEER_FEED_TOOL_OUTPUT_MAX_BYTES;

/// Cut `output` to at most `max_lines` lines and `max_bytes` bytes, keeping
/// the TAIL and only ever whole lines. Returns the kept suffix (its newlines
/// intact) and how many lines were dropped off the FRONT; the caller decides
/// how to mark the cut. `(output, 0)` when it already fits.
///
/// A trailing line without its newline counts as a line and is kept like any
/// other — a command that never printed a final newline still shows its last
/// word.
pub fn truncate_output(output: &str, max_lines: usize, max_bytes: usize) -> (String, usize) {
    let lines: Vec<&str> = output.split_inclusive('\n').collect();
    let mut kept_bytes = 0usize;
    let mut kept_lines = 0usize;
    // Walk BACKWARDS: the first line that does not fit ends the kept part,
    // and every line before it is omitted even when it would fit on its own
    // (a SUFFIX, mirroring the patch cut's prefix).
    for line in lines.iter().rev() {
        if kept_lines >= max_lines || kept_bytes + line.len() > max_bytes {
            break;
        }
        kept_bytes += line.len();
        kept_lines += 1;
    }
    let omitted = lines.len() - kept_lines;
    if omitted == 0 {
        return (output.to_string(), 0);
    }
    (lines[omitted..].concat(), omitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_that_fits_is_untouched() {
        assert_eq!(truncate_output("a\nb\nc\n", 200, 16_384), ("a\nb\nc\n".to_string(), 0));
        assert_eq!(truncate_output("", 200, 16_384), (String::new(), 0));
    }

    #[test]
    fn the_cut_keeps_the_tail_and_counts_the_front() {
        // Line cap: the LAST two lines, two dropped.
        assert_eq!(truncate_output("a\nb\nc\nd\n", 2, 16_384), ("c\nd\n".to_string(), 2));
        // Byte cap: a line that would cross it is dropped WHOLE, never split.
        assert_eq!(truncate_output("aaaa\nbb\n", 200, 5), ("bb\n".to_string(), 1));
        // A cap smaller than the last line keeps nothing at all.
        assert_eq!(truncate_output("aaaa\nbbbb\n", 200, 2), (String::new(), 2));
    }

    #[test]
    fn a_missing_final_newline_still_counts_as_a_line() {
        assert_eq!(truncate_output("a\nb", 1, 100), ("b".to_string(), 1));
        assert_eq!(truncate_output("only", 1, 100), ("only".to_string(), 0));
    }

    #[test]
    fn a_zero_cap_keeps_nothing() {
        assert_eq!(truncate_output("a\nb\n", 0, 16_384), (String::new(), 2));
    }
}
