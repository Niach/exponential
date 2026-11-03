//! VAPP-239 (eApps M2, `packages/emcp/fixtures/upstream.json` `device`): the
//! device's half of a DEVICE-ROUTED upstream call. The relay sends
//! `upstream_call {callId}` down this device's control socket; only the id
//! crosses it. The device claims the call over its own authenticated API
//! (`mcpServers.claimDeviceCall`, as itself), runs it through
//! [`emcp::device_call`] (http, or the stdio command spawned here) and seals
//! the answer to the call's reply key (enet-core `push_seal`), so the relay
//! carries opaque bytes and cannot forge an answer: the key never crossed
//! it. A call that does not claim (another member's, expired, replayed) is
//! dropped: the server's wait times out on its own.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

use crate::control_channel::ControlApi;

/// How long the device gives one call (connect, list, call), under the
/// relay's 25 s wait (`device.answerTimeoutMs`).
pub const DEVICE_CALL_TIMEOUT: Duration = Duration::from_secs(20);

/// `{t:"upstream_call", callId}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamCallFrame {
    pub call_id: String,
}

impl UpstreamCallFrame {
    pub fn parse(text: &str) -> Option<Self> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            t: String,
            call_id: String,
        }
        let wire: Wire = serde_json::from_str(text).ok()?;
        (wire.t == "upstream_call" && !wire.call_id.is_empty() && wire.call_id.len() <= 64).then_some(UpstreamCallFrame { call_id: wire.call_id })
    }
}

/// `{t:"upstream_answer", callId, sealed}`.
pub fn answer_frame(call_id: &str, sealed: &str) -> String {
    serde_json::json!({ "t": "upstream_answer", "callId": call_id, "sealed": sealed }).to_string()
}

/// Seal `answer` to the call's reply key (base64url X25519): the frame's
/// `sealed`. None when the key is malformed.
pub fn seal_answer(reply_public: &str, answer: &emcp::contract::upstream::DeviceAnswer) -> Option<String> {
    let key = data_encoding::BASE64URL_NOPAD.decode(reply_public.trim_end_matches('=').as_bytes()).ok()?;
    let key: [u8; 32] = key.try_into().ok()?;
    let body = serde_json::to_vec(answer).ok()?;
    let sealed = enet_core::push_seal(&key, &body).ok()?;
    Some(data_encoding::BASE64URL_NOPAD.encode(&sealed))
}

/// Claim, run and answer one call, off the socket loop (blocking work on a
/// blocking thread). The answer goes up the control socket it came down.
pub fn spawn(call_id: String, device_id: String, api: Arc<dyn ControlApi>, out_tx: flume::Sender<String>) {
    tokio::task::spawn_blocking(move || {
        let claimed = match api.claim_upstream_call(&call_id, &device_id) {
            Ok(value) => value,
            Err(err) => {
                log::info!("steer control: upstream_call {call_id} not claimed: {err}");
                return;
            }
        };
        let call: emcp::device_call::ClaimedCall = match serde_json::from_value(claimed) {
            Ok(call) => call,
            Err(err) => {
                log::warn!("steer control: upstream_call {call_id}: the claim does not parse ({err})");
                return;
            }
        };
        let answer = emcp::device_call::run_device_call(&call, DEVICE_CALL_TIMEOUT);
        log::info!("steer control: upstream_call {call_id} answered ({})", if matches!(answer, emcp::contract::upstream::DeviceAnswer::Ok { .. }) { "ok" } else { "refused or failed" });
        match seal_answer(&call.reply_public, &answer) {
            Some(sealed) => {
                let _ = out_tx.send(answer_frame(&call_id, &sealed));
            }
            None => log::warn!("steer control: upstream_call {call_id}: the reply key is malformed"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use emcp::contract::upstream::DeviceAnswer;

    #[test]
    fn the_frame_parses_only_an_upstream_call() {
        assert_eq!(
            UpstreamCallFrame::parse(r#"{"t":"upstream_call","callId":"c1"}"#),
            Some(UpstreamCallFrame { call_id: "c1".into() })
        );
        assert_eq!(UpstreamCallFrame::parse(r#"{"t":"tunnel_open","callId":"c1"}"#), None);
        assert_eq!(UpstreamCallFrame::parse(r#"{"t":"upstream_call","callId":""}"#), None);
    }

    #[test]
    fn the_answer_opens_only_with_the_reply_secret() {
        let pair = enet_core::PushKeypair::generate();
        let public = data_encoding::BASE64URL_NOPAD.encode(&pair.public());
        let answer = DeviceAnswer::ok(serde_json::json!({"content": [{"type": "text", "text": "21.5"}]}));
        let sealed = seal_answer(&public, &answer).unwrap();
        let bytes = data_encoding::BASE64URL_NOPAD.decode(sealed.as_bytes()).unwrap();
        let opened = enet_core::push_open(&pair.secret(), &bytes).unwrap();
        assert_eq!(serde_json::from_slice::<DeviceAnswer>(&opened).unwrap(), answer);
        assert!(enet_core::push_open(&enet_core::PushKeypair::generate().secret(), &bytes).is_err());
        assert_eq!(seal_answer("short", &answer), None);
        let frame: serde_json::Value = serde_json::from_str(&answer_frame("c1", &sealed)).unwrap();
        assert_eq!(frame["t"], "upstream_answer");
        assert_eq!(frame["callId"], "c1");
    }
}
