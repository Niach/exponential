//! The sandbox helper for this crate's own tests: the app and the daemon
//! re-enter their own executable with `__ci-sandbox` instead.

fn main() {
    let code = ci_runner::sandbox_main().unwrap_or_else(|| {
        eprintln!("usage: ci-sandbox-helper {} [--hide DIR]... [--hide-file FILE]... -- COMMAND...", ci_runner::SANDBOX_ARG);
        2
    });
    std::process::exit(code);
}
