//! Serve the real endpoint for a manual agent test: prints the URL and a
//! token for one run, then stays up.
//! `cargo run -p computer --example serve`

fn main() {
    let grant = computer::grant("manual-run").expect("computer use is unavailable here");
    println!("{}\n{}", grant.url, grant.token);
    computer::on_first_action(|session| eprintln!("first action of {session}"));
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Some(session) = computer::driving() {
            eprintln!("driving: {session}");
        }
    }
}
