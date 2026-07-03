//! The per-app, per-account device-presence socket (masterplan-v3 §8.3).
//!
//! What makes "Start on my desktop" work: phone → `steer.myDevices()` → relay
//! `devices` map (populated by OUR `online` frame) → `steer.startSession` →
//! relay routes a `start_session` frame down this socket → the §7 launcher.
//!
//! The channel holds **no** PTY and **no** session state — a thin presence
//! beacon. Exactly one per account per app process; multi-window shares it
//! (account-scoped, not window-scoped).
//!
//! Lifecycle (§8.3):
//! 1. gate on `steer.config()` — `{enabled:false}` is a NORMAL state (EXP-4):
//!    do nothing except a slow **15-minute** recheck. The enabled verdict is
//!    then cached for the life of the loop — reconnects go straight to
//!    `mintTicket`, whose own `{disabled}` result re-routes to the slow poll
//!    (that mint-side answer IS the config cache refresh).
//! 2. persistent `deviceId` + hostname `deviceLabel` (owned by the caller —
//!    [`crate::persistent_device_id`] / `api::users::hostname()`).
//! 3. mint control ticket → dial the returned URL as-is → send `online`.
//! 4. inbound `start_session {issueId}` → hand to the launcher callback (the
//!    callback marshals to the gpui foreground itself). `bye`/`error`/`kill`
//!    on this socket are logged; `kill` is not session-scoped here → no-op.
//! 5. reconnect with exponential backoff (250ms → 30s cap, full jitter);
//!    each attempt re-mints (tickets last ~60s). Backoff resets after a
//!    connection lived ≥ [`crate::BACKOFF_RESET_AFTER`] (outliving the ticket
//!    window proves the path — there is no `online` ack frame to key on).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use api::error::ApiError;
use api::trpc::TrpcClient;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::frames::{ClientFrame, ServerFrame};
use crate::{dial, Backoff, SteerRuntime, BACKOFF_RESET_AFTER};

/// §8.3 #6: the slow recheck cadence while the instance reports steer off.
pub const DISABLED_RECHECK: Duration = Duration::from_secs(15 * 60);

/// The stable device identity this channel announces (§8.2).
#[derive(Clone, Debug)]
pub struct DeviceIdentity {
    /// Install-persistent UUID ([`crate::persistent_device_id`]).
    pub device_id: String,
    /// OS hostname (`api::users::hostname()`) — the phone picker's label.
    pub device_label: String,
}

/// The two server calls the channel needs, injectable for tests. Blocking
/// (ureq underneath) — the loop wraps calls in `spawn_blocking`.
pub trait ControlApi: Send + Sync + 'static {
    /// `steer.config().enabled` — `Ok(false)` = disabled (normal, §8.3 #1).
    fn config_enabled(&self) -> Result<bool, ApiError>;
    /// `steer.mintTicket({kind:"control", deviceLabel})` → dial URL, or
    /// `Ok(None)` when the instance reports disabled.
    fn mint_control(&self, device_label: &str) -> Result<Option<String>, ApiError>;
}

/// Production [`ControlApi`] over the account's tRPC client.
pub struct TrpcControlApi(pub Arc<TrpcClient>);

impl ControlApi for TrpcControlApi {
    fn config_enabled(&self) -> Result<bool, ApiError> {
        Ok(api::steer::config(&self.0)?.enabled)
    }

    fn mint_control(&self, device_label: &str) -> Result<Option<String>, ApiError> {
        Ok(api::steer::mint_control_ticket(&self.0, Some(device_label))?
            .into_ticket()
            .map(|ticket| ticket.url))
    }
}

/// The launcher trigger (§8.3 #4): receives the `issueId` of an inbound
/// `start_session`. Runs on the steer runtime — implementations marshal to
/// the gpui foreground themselves (e.g. via a flume channel the app drains).
pub type StartSessionFn = Arc<dyn Fn(String) + Send + Sync>;

/// Stop handle for the channel task. Dropping it does NOT stop the task —
/// call [`ControlChannelHandle::stop`] (sign-out / account switch).
pub struct ControlChannelHandle {
    stopped: Arc<AtomicBool>,
    stop_tx: flume::Sender<()>,
}

impl ControlChannelHandle {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.stop_tx.send(());
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

/// Spawn the per-account control channel onto the steer runtime (§8.3 #1:
/// `crates/app` calls this on login / active-account change).
pub fn spawn_control_channel(
    runtime: &SteerRuntime,
    device: DeviceIdentity,
    control_api: Arc<dyn ControlApi>,
    on_start_session: StartSessionFn,
) -> ControlChannelHandle {
    let stopped = Arc::new(AtomicBool::new(false));
    let (stop_tx, stop_rx) = flume::bounded::<()>(1);
    let handle = ControlChannelHandle {
        stopped: stopped.clone(),
        stop_tx,
    };
    runtime.handle().spawn(run_control_loop(
        device,
        control_api,
        on_start_session,
        stopped,
        stop_rx,
    ));
    handle
}

// ---------------------------------------------------------------------------
// The pure state machine (unit-tested; the loop below just executes it)
// ---------------------------------------------------------------------------

/// What just happened to the channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ControlEvent {
    /// `steer.config()` said disabled, or `mintTicket` returned `{disabled}`.
    Disabled,
    /// config/mint transport error (server unreachable, 5xx, …).
    ApiError,
    /// The dial or the `online` send failed.
    ConnectFailed,
    /// The socket dropped after living this long.
    Disconnected { lived: Duration },
}

/// What the loop must do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ControlAction {
    /// Sleep the disabled slow-poll (15 min), then re-check `config()`.
    RecheckAfter(Duration),
    /// Sleep a backoff delay, then re-mint + re-dial.
    RetryAfter(Duration),
}

/// §8.3 policy: map an event to the next action, mutating the backoff.
pub(crate) fn next_action(event: ControlEvent, backoff: &mut Backoff) -> ControlAction {
    match event {
        ControlEvent::Disabled => {
            backoff.reset();
            ControlAction::RecheckAfter(DISABLED_RECHECK)
        }
        ControlEvent::ApiError | ControlEvent::ConnectFailed => {
            ControlAction::RetryAfter(backoff.next_delay())
        }
        ControlEvent::Disconnected { lived } => {
            if lived >= BACKOFF_RESET_AFTER {
                backoff.reset();
            }
            ControlAction::RetryAfter(backoff.next_delay())
        }
    }
}

// ---------------------------------------------------------------------------
// The IO loop
// ---------------------------------------------------------------------------

async fn run_control_loop(
    device: DeviceIdentity,
    control_api: Arc<dyn ControlApi>,
    on_start_session: StartSessionFn,
    stopped: Arc<AtomicBool>,
    stop_rx: flume::Receiver<()>,
) {
    let mut backoff = Backoff::control();

    // §8.3 #1 — the config gate, checked once up front and again on every
    // return to the disabled slow-poll. Between reconnects the cached verdict
    // stands; mintTicket's `{disabled}` answer refreshes it for free.
    let mut check_config = true;

    while !stopped.load(Ordering::SeqCst) {
        if check_config {
            let api_for_config = control_api.clone();
            let enabled =
                tokio::task::spawn_blocking(move || api_for_config.config_enabled()).await;
            match enabled {
                Ok(Ok(true)) => {
                    check_config = false;
                }
                Ok(Ok(false)) => {
                    let action = next_action(ControlEvent::Disabled, &mut backoff);
                    if sleep_action(action, &stop_rx).await.is_break() {
                        return;
                    }
                    continue;
                }
                Ok(Err(err)) => {
                    log::debug!("steer control: config check failed: {err}");
                    let action = next_action(ControlEvent::ApiError, &mut backoff);
                    if sleep_action(action, &stop_rx).await.is_break() {
                        return;
                    }
                    continue;
                }
                Err(join_err) => {
                    log::warn!("steer control: config task panicked: {join_err}");
                    return;
                }
            }
        }

        // §8.3 #3 — mint, then dial IMMEDIATELY (§8.7: < 5s budget).
        let api_for_mint = control_api.clone();
        let label = device.device_label.clone();
        let minted =
            tokio::task::spawn_blocking(move || api_for_mint.mint_control(&label)).await;
        let url = match minted {
            Ok(Ok(Some(url))) => url,
            Ok(Ok(None)) => {
                // Instance flipped to disabled → back to the slow poll.
                check_config = true;
                let action = next_action(ControlEvent::Disabled, &mut backoff);
                if sleep_action(action, &stop_rx).await.is_break() {
                    return;
                }
                continue;
            }
            Ok(Err(ApiError::Unauthorized)) => {
                // Dead session token: the account is being torn down by the
                // §5.6b path — park on the slow poll rather than hammering.
                log::info!("steer control: unauthorized; slow-polling until re-login");
                check_config = true;
                let action = next_action(ControlEvent::Disabled, &mut backoff);
                if sleep_action(action, &stop_rx).await.is_break() {
                    return;
                }
                continue;
            }
            Ok(Err(err)) => {
                log::debug!("steer control: mint failed: {err}");
                let action = next_action(ControlEvent::ApiError, &mut backoff);
                if sleep_action(action, &stop_rx).await.is_break() {
                    return;
                }
                continue;
            }
            Err(join_err) => {
                log::warn!("steer control: mint task panicked: {join_err}");
                return;
            }
        };

        let event = match connect_and_listen(&url, &device, &on_start_session, &stop_rx).await
        {
            ConnectionOutcome::Stopped => return,
            ConnectionOutcome::ConnectFailed(reason) => {
                log::debug!("steer control: connect failed: {reason}");
                ControlEvent::ConnectFailed
            }
            ConnectionOutcome::Dropped { lived } => {
                log::debug!("steer control: socket dropped after {lived:?}");
                ControlEvent::Disconnected { lived }
            }
        };
        let action = next_action(event, &mut backoff);
        if sleep_action(action, &stop_rx).await.is_break() {
            return;
        }
    }
}

enum ConnectionOutcome {
    Stopped,
    ConnectFailed(String),
    Dropped { lived: Duration },
}

async fn connect_and_listen(
    url: &str,
    device: &DeviceIdentity,
    on_start_session: &StartSessionFn,
    stop_rx: &flume::Receiver<()>,
) -> ConnectionOutcome {
    let mut ws = match dial(url).await {
        Ok(stream) => stream,
        // A 401 upgrade rejection on the control path is handled like any
        // connect failure — every retry re-mints anyway (§8.3 #5).
        Err(crate::DialError::Unauthorized) => {
            return ConnectionOutcome::ConnectFailed("ticket rejected (401)".to_string())
        }
        Err(crate::DialError::Other(reason)) => return ConnectionOutcome::ConnectFailed(reason),
    };

    // §8.3 #3 — announce presence immediately on open.
    let online = ClientFrame::Online {
        device_id: &device.device_id,
        device_label: Some(&device.device_label),
    }
    .to_json();
    if let Err(err) = ws.send(Message::Text(online)).await {
        return ConnectionOutcome::ConnectFailed(crate::redact_ticket(&err.to_string()));
    }
    log::info!(
        "steer control: online as {} ({})",
        device.device_label,
        device.device_id
    );
    let established = Instant::now();

    loop {
        tokio::select! {
            _ = stop_rx.recv_async() => {
                let _ = ws.close(None).await;
                return ConnectionOutcome::Stopped;
            }
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(text))) => match ServerFrame::parse(&text) {
                    // §8.3 #4 — the one frame we act on.
                    Some(ServerFrame::StartSession { issue_id }) => {
                        log::info!("steer control: remote start_session for issue {issue_id}");
                        on_start_session(issue_id);
                    }
                    Some(other) => {
                        // bye/error/kill here: logged; kill is not
                        // session-scoped on the control socket → no-op.
                        log::debug!("steer control: ignoring frame {other:?}");
                    }
                    None => log::debug!("steer control: unparseable frame ignored"),
                },
                Some(Ok(Message::Close(frame))) => {
                    log::debug!("steer control: closed by relay: {frame:?}");
                    return ConnectionOutcome::Dropped { lived: established.elapsed() };
                }
                Some(Ok(_binary_or_ping)) => {
                    // Binary frames never target the control socket; tungstenite
                    // answers pings internally.
                }
                Some(Err(err)) => {
                    log::debug!("steer control: socket error: {err}");
                    return ConnectionOutcome::Dropped { lived: established.elapsed() };
                }
                None => return ConnectionOutcome::Dropped { lived: established.elapsed() },
            }
        }
    }
}

/// Interruptible sleep: `Break` when the handle was stopped mid-sleep.
async fn sleep_action(
    action: ControlAction,
    stop_rx: &flume::Receiver<()>,
) -> std::ops::ControlFlow<()> {
    let delay = match action {
        ControlAction::RecheckAfter(d) | ControlAction::RetryAfter(d) => d,
    };
    tokio::select! {
        _ = tokio::time::sleep(delay) => std::ops::ControlFlow::Continue(()),
        _ = stop_rx.recv_async() => std::ops::ControlFlow::Break(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_routes_to_the_15_minute_slow_poll_and_resets_backoff() {
        let mut backoff = Backoff::control();
        // Grow the backoff first so the reset is observable.
        backoff.next_delay();
        backoff.next_delay();
        assert!(backoff.bound() > Duration::from_millis(250));
        let action = next_action(ControlEvent::Disabled, &mut backoff);
        assert_eq!(action, ControlAction::RecheckAfter(DISABLED_RECHECK));
        assert_eq!(backoff.bound(), Duration::from_millis(250));
    }

    #[test]
    fn api_and_connect_failures_back_off_exponentially() {
        let mut backoff = Backoff::control();
        let mut bounds = Vec::new();
        for event in [
            ControlEvent::ApiError,
            ControlEvent::ConnectFailed,
            ControlEvent::ApiError,
        ] {
            bounds.push(backoff.bound());
            match next_action(event, &mut backoff) {
                ControlAction::RetryAfter(delay) => assert!(delay <= *bounds.last().unwrap()),
                other => panic!("expected RetryAfter, got {other:?}"),
            }
        }
        assert_eq!(
            bounds,
            vec![
                Duration::from_millis(250),
                Duration::from_millis(500),
                Duration::from_secs(1)
            ]
        );
    }

    #[test]
    fn short_lived_connection_keeps_growing_backoff() {
        let mut backoff = Backoff::control();
        for _ in 0..4 {
            next_action(
                ControlEvent::Disconnected {
                    lived: Duration::from_secs(5),
                },
                &mut backoff,
            );
        }
        assert_eq!(backoff.bound(), Duration::from_secs(4), "250ms << 4");
    }

    #[test]
    fn long_lived_connection_resets_backoff_before_retry() {
        // The >60s-lived rule (§8.3 #5): outliving the ticket window proves
        // the path; the NEXT retry starts from base again.
        let mut backoff = Backoff::control();
        for _ in 0..6 {
            backoff.next_delay();
        }
        assert!(backoff.bound() >= Duration::from_secs(8));
        let action = next_action(
            ControlEvent::Disconnected {
                lived: BACKOFF_RESET_AFTER,
            },
            &mut backoff,
        );
        match action {
            ControlAction::RetryAfter(delay) => {
                assert!(delay <= Duration::from_millis(250), "sampled from base");
            }
            other => panic!("expected RetryAfter, got {other:?}"),
        }
        assert_eq!(backoff.bound(), Duration::from_millis(500), "base doubled once");
    }

    #[test]
    fn stop_handle_flips_stopped() {
        let stopped = Arc::new(AtomicBool::new(false));
        let (stop_tx, stop_rx) = flume::bounded::<()>(1);
        let handle = ControlChannelHandle {
            stopped: stopped.clone(),
            stop_tx,
        };
        assert!(!handle.is_stopped());
        handle.stop();
        assert!(handle.is_stopped());
        assert!(stop_rx.try_recv().is_ok(), "sleepers are woken");
    }
}
