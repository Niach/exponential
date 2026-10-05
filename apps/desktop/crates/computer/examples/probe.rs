//! Manual probe of the real backend, off the main thread like the server:
//! `cargo run -p computer --example probe -- [shot <path>] [ui] [click X Y] [type TEXT] [key CHORD] [focus ID]`.

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
                "shot" => {
                    let path = next();
                    guard.screenshot(Target::Display(0), Some(path.as_ref())).map(|(output, map)| {
                        mapping = Some(map);
                        output
                    })
                }
                "ui" => guard.read_ui(mapping, None),
                "click" => {
                    let (x, y) = (next().parse().unwrap(), next().parse().unwrap());
                    guard.click(mapping, x, y, Button::Left, 1)
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
