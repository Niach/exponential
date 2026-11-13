//! `git-remote-exp <remote> <url>`: git runs this for `exp://` URLs. The
//! standalone binary authenticates from the environment
//! (`EXP_GIT_SERVER` + `EXP_GIT_TOKEN`, a repo token, or `EXP_SERVER` +
//! `EXP_SESSION_TOKEN`, a session the server mints one from). The CLI and
//! the desktop app run the same code with their own signed-in account.

fn main() -> std::process::ExitCode {
    git_remote_exp::main_with(git_remote_exp::auth_from_env)
}
