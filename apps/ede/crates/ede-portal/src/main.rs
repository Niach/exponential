//! `xdg-desktop-portal-exponential`: D-Bus activated by the portal frontend inside
//! an eDE session (`/usr/libexec/xdg-desktop-portal-exponential`).

use std::{process::ExitCode, sync::Arc};

fn main() -> ExitCode {
    // A crash-monitor process (ede-observability) runs and exits here.
    ede_observability::run_crash_monitor_if_requested();
    // #ERR-29: the compositor's diagnostics (panics, errors, native
    // crashes), tagged `portal`.
    let _diagnostics = ede_observability::init("portal", "info", ede_observability::Output::Stderr);
    ede_observability::watch_native_crashes("portal");
    match serve() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "the portal stopped");
            ExitCode::FAILURE
        }
    }
}

fn serve() -> Result<(), Box<dyn std::error::Error>> {
    // #EDE-15: the root token only mints; every portal connection holds
    // the narrow grant (capture, the person's input for RemoteDesktop,
    // ui for shortcuts) and nothing more.
    let token = ede_portal::minted_token().map_err(|e| format!("ede-protocol: {e}"))?;
    let connect: ede_portal::Connect = Arc::new(move || ede_protocol::Client::connect_with_token(token.clone()));
    let ctx = ede_portal::Ctx::new(connect, ede_portal::config::Config::path());
    zbus::block_on(async {
        let _conn = ede_portal::serve(zbus::connection::Builder::session()?, ctx).await?;
        tracing::info!("serving {}", ede_portal::BUS_NAME);
        std::future::pending::<()>().await;
        Ok::<_, zbus::Error>(())
    })?;
    Ok(())
}
