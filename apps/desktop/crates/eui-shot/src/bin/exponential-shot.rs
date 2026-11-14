//! `exponential-shot <bundle.json> <out.png>` (#FEED-244): paint one app
//! screen bundle headless to a PNG. Exit 0 = the PNG is written; exit 2 =
//! the refusal is the last stderr line (the device sends it back verbatim);
//! `--probe` = exit 0 when this machine has a headless renderer.

use std::process::ExitCode;

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--probe") {
        return match gpui_platform::current_headless_renderer() {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("this device has no GPU renderer for a headless picture ({e:#})")),
        };
    }
    let [bundle, out] = args.as_slice() else {
        return fail("usage: exponential-shot <bundle.json> <out.png> | --probe");
    };
    let text = match std::fs::read_to_string(bundle) {
        Ok(text) => text,
        Err(e) => return fail(&format!("read {bundle}: {e}")),
    };
    let value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => return fail(&format!("the bundle is not JSON: {e}")),
    };
    let result = eui_shot::parse_bundle(&value).and_then(|b| eui_shot::render(&b)).and_then(|image| eui_shot::png(&image));
    match result {
        Ok(bytes) => match std::fs::write(out, bytes) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("write {out}: {e}")),
        },
        Err(message) => fail(&message),
    }
}
