//! `xdg-desktop-portal-ede`: D-Bus activated by the portal frontend inside
//! an eDE session (`/usr/libexec/xdg-desktop-portal-ede`).

use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let connect: ede_portal::Connect = Arc::new(ede_protocol::Client::connect_env);
    let ctx = ede_portal::Ctx::new(connect, ede_portal::config::Config::path());
    zbus::block_on(async {
        let _conn = ede_portal::serve(zbus::connection::Builder::session()?, ctx).await?;
        tracing::info!("serving {}", ede_portal::BUS_NAME);
        std::future::pending::<()>().await;
        Ok::<_, zbus::Error>(())
    })?;
    Ok(())
}
