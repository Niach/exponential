//! EXP-1324: `code-block` (General components): THE CodeBlock at size sm —
//! the REAL painter (`crate::code_block`) — the web island
//! (`apps/styleguide/src/entries/code-block.tsx`), identical ×4: a titled
//! plain block, a command's Output (`ansi`) and a failed call's destructive
//! Output.

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

use crate::code_block::{code_block, CodeBlockProps};

pub(crate) const ID: &str = "code-block";
pub(crate) const OWNER: &str = "EXP-1324";

const TS: &str = "export function coldStart(board: Board): Promise<void> {\n  // Paint the cached snapshot first, subscribe after the first frame.\n  const cached = snapshots.read(board.id)\n  return render(cached ?? [], { deferSubscribe: true })\n}";

const ANSI: &str = "$ ./gradlew :app:startupProfile\n> Task :app:startupProfile\n\u{1b}[33mwarning:\u{1b}[0m baseline profile is 3 days old\nPixel 6a     p50 740 ms   p90 812 ms\n\u{1b}[32mBUILD SUCCESSFUL\u{1b}[0m in 41s";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let block = |id: &str, title: &str, code: &str, language: &'static str, destructive: bool, cx: &App| {
        code_block(
            CodeBlockProps {
                id: SharedString::from(format!("styleguide-code-block-{id}")),
                title: Some(SharedString::from(title.to_string())),
                code: code.to_string(),
                language,
                destructive,
                copyable: true,
            },
            cx,
        )
    };
    let output = steer::trace::copy::OUTPUT;
    div()
        .w(px(672.))
        .flex()
        .flex_col()
        .gap_4()
        .child(block("plain", "startup.ts", TS, "plain", false, cx))
        .child(block("ansi", output, ANSI, "ansi", false, cx))
        .child(block("destructive", output, super::trace::BENCHMARK_OUTPUT, "ansi", true, cx))
}
