//! Manual probe of the real backend, off the main thread like the server:
//! `cargo run -p computer --example probe -- [shot <path>] [display N <path>] [window ID <path>]
//! [ui] [uiwindow ID] [click X Y] [scroll X Y DY] [type TEXT] [key CHORD] [focus ID]`.

use computer::backend::{self, Button, Target};
use computer::guard::Guard;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    computer::refresh_key_layout();
    std::thread::spawn(move || {
        let guard = Guard::new(backend::platform().expect("backend"));
        println!("readiness: {:?}", guard.readiness(false));
        println!("{}", guard.list_windows().map(|o| o.text).unwrap_or_else(|e| e));
        let mut mapping = None;
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
                    guard.click(mapping, x, y, Button::Left, 1)
                }
                "scroll" => {
                    let (x, y, dy) =
                        (next().parse().unwrap(), next().parse().unwrap(), next().parse().unwrap());
                    guard.scroll(mapping, x, y, 0, dy)
                }
                "type" => guard.type_text(&next()),
                "key" => guard.key(&next()),
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
