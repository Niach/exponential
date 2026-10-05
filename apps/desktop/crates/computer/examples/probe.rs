//! Manual probe of the real backend, off the main thread like the server:
//! `cargo run -p computer --example probe -- [shot <path>] [display N <path>] [window ID <path>]
//! [ui] [uiwindow ID] [click X Y] [scroll X Y DY] [type TEXT] [key CHORD] [focus ID]
//! [delivery auto|foreground|background] [target ID] [sleep MS]`. `delivery` and `target` hold
//! for the steps after them (default auto, like the tools: background on macOS when a target
//! window is known; target = the background window, default: the last window shot, else the
//! one under the click or the focused one).

use computer::backend::{self, Button, Delivery, Target};
use computer::guard::Guard;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    computer::refresh_key_layout();
    std::thread::spawn(move || {
        let guard = Guard::new(backend::platform().expect("backend"));
        println!("readiness: {:?}", guard.readiness(false));
        println!("{}", guard.list_windows().map(|o| o.text).unwrap_or_else(|e| e));
        let mut mapping = None;
        let mut delivery = Delivery::Auto;
        let mut target: Option<u32> = None;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let mut next = || args.next().cloned().unwrap_or_default();
            let outcome = match arg.as_str() {
                "shot" | "display" | "window" => {
                    let target = match arg.as_str() {
                        "display" => Target::Display(next().parse().unwrap()),
                        "window" => Target::Window(next().parse().unwrap()),
                        _ => Target::Display(0),
                    };
                    let path = next();
                    guard.screenshot(target, Some(path.as_ref())).map(|(output, map)| {
                        mapping = Some(map);
                        output
                    })
                }
                "ui" => guard.read_ui(mapping, None),
                "uiwindow" => guard.read_ui(mapping, Some(next().parse().unwrap())),
                "click" => {
                    let (x, y) = (next().parse().unwrap(), next().parse().unwrap());
                    guard.click(mapping, x, y, Button::Left, 1, delivery, target)
                }
                "scroll" => {
                    let (x, y, dy) =
                        (next().parse().unwrap(), next().parse().unwrap(), next().parse().unwrap());
                    guard.scroll(mapping, x, y, 0, dy, delivery, target)
                }
                "type" => guard.type_text(&next(), delivery, target, mapping),
                "key" => guard.key(&next(), delivery, target, mapping),
                "delivery" => {
                    let name = next();
                    Delivery::parse(Some(name.as_str()).filter(|name| *name != "auto"))
                }
                .map(|parsed| {
                    delivery = parsed;
                    computer::guard::ToolOutput { text: format!("{parsed:?}"), ..Default::default() }
                }),
                "target" => {
                    target = next().parse().ok();
                    Ok(computer::guard::ToolOutput { text: format!("{target:?}"), ..Default::default() })
                }
                "sleep" => {
                    std::thread::sleep(std::time::Duration::from_millis(next().parse().unwrap()));
                    Ok(computer::guard::ToolOutput::default())
                }
                "focus" => guard.focus_window(next().parse().unwrap()),
                other => Err(format!("unknown step {other}")),
            };
            match outcome {
                Ok(output) => println!("{arg}: {}", output.text),
                Err(error) => println!("{arg}: ERROR {error}"),
            }
        }
    })
    .join()
    .unwrap();
}
