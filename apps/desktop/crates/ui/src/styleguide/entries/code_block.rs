//! EXP-1324: `code-block` (General components): THE CodeBlock at size sm —
//! the REAL painter (`crate::code_block`): a command's Output (`ansi`), a
//! failed call's destructive block, and plain code without a header.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

use crate::code_block::{code_block, CodeBlockProps};

pub(crate) const ID: &str = "code-block";
pub(crate) const OWNER: &str = "EXP-1324";

const FAILED: &str = "error: test failed, to rerun pass `-p engine --lib`\nthread 'startup::cold_start' panicked at src/startup.rs:42:9:\nassertion `left < right` failed: 812 < 800";

const RUST: &str = "fn cold_start(profile: &Profile) -> Duration {\n    // The snapshot cache persists on ON_STOP.\n    profile.p50().min(Duration::from_millis(800))\n}";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let block = |id: &str, title: Option<&str>, code: &str, language: &'static str, destructive: bool, cx: &App| {
        code_block(
            CodeBlockProps {
                id: SharedString::from(format!("styleguide-code-block-{id}")),
                title: title.map(SharedString::from),
                code: code.to_string(),
                language,
                destructive,
                copyable: title.is_some(),
            },
            cx,
        )
    };
    div()
        .w(px(640.))
        .flex()
        .flex_col()
        .gap_4()
        .child(block("ansi", Some(steer::trace::copy::OUTPUT), super::trace::GRADLE_OUTPUT, "ansi", false, cx))
        .child(block("destructive", Some(steer::trace::copy::OUTPUT), FAILED, "ansi", true, cx))
        .child(block("rust", None, RUST, "rust", false, cx))
}
