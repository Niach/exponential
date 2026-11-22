//! `xdg-desktop-portal-exponential`: D-Bus activated by the portal frontend inside
//! an eDE session (`/usr/libexec/xdg-desktop-portal-exponential`).

use std::{process::ExitCode, sync::Arc};

fn main() -> ExitCode {
    // A crash-monitor process (ede-observability) runs and exits here.
    ede_observability::run_crash_monitor_if_requested();
    // #EDE-172 (P7): the channel the compositor started the portal with,
    // before the crash monitor or any thread starts.
    let channel = ede_portal::take_channel();
    // #ERR-29: the compositor's diagnostics (panics, errors, native
    // crashes), tagged `portal`.
    let _diagnostics = ede_observability::init("portal", "info", ede_observability::Output::Stderr);
    ede_observability::watch_native_crashes("portal");
    match serve(channel) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "the portal stopped");
            ExitCode::FAILURE
        }
    }
}

fn serve(channel: Result<Option<ede_protocol::Client>, ede_protocol::Error>) -> Result<(), Box<dyn std::error::Error>> {
    let connect: ede_portal::Connect = match channel.map_err(|e| format!("the inherited channel: {e}"))? {
        // #EDE-172 (P7): started by the compositor: its grant on its channel,
        // and after that first roundtrip no other process of the person
        // reads this one.
        Some(channel) => {
            if let Err(e) = ede_protocol::process::refuse_ptrace() {
                tracing::warn!("PR_SET_DUMPABLE: {e}");
            }
            ede_portal::on_channel(channel)
        }
        // The compositor starts the portal itself and its copy owns the bus
        // name; one D-Bus activation started anyway is not it.
        None if ede_protocol::process::helpers_spawned() => {
            return Err("no channel: this session's compositor starts the portal itself ([session] helpers = \"compositor\"); a portal started another way is not it".into());
        }
        // #EDE-15: the root token only mints; every portal connection holds
        // the narrow grant (capture, the person's input for RemoteDesktop,
        // ui for shortcuts) and nothing more.
        None => {
            let token = ede_portal::minted_token().map_err(|e| format!("ede-protocol: {e}"))?;
            Arc::new(move || ede_protocol::Client::connect_with_token(token.clone()))
        }
    };
    let ctx = ede_portal::Ctx::new(connect, ede_portal::config::Config::path());
    zbus::block_on(async {
        let _conn = ede_portal::serve(zbus::connection::Builder::session()?, ctx).await?;
        tracing::info!("serving {}", ede_portal::BUS_NAME);
        std::future::pending::<()>().await;
        Ok::<_, zbus::Error>(())
    })?;
    Ok(())
}
