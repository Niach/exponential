//! Signaling envelopes. ONE JSON shape shared by the web page (peer.js), the CLI, and the
//! iOS/Android hello apps. Transported opaque through the steer relay:
//!   viewer → publisher : {"t":"input","data":"<envelope>"}                      (≤ 8 KiB)
//!   publisher → viewer : {"t":"activity","seq":N,"event":{"kind":"narration","text":"<envelope>"}} (≤ 16 KiB)
//! Every envelope carries `from` (a random peer id) and, when addressed, `to`, so several
//! viewers can share one room.
use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Body {
    /// Signed SDP offer. `sig` = Ed25519 over [`signing::signed_bytes`].
    Offer {
        sdp: String,
        #[serde(rename = "pub")]
        pubkey: String,
        sig: String,
    },
    /// Signed SDP answer, same envelope as the offer.
    Answer {
        sdp: String,
        #[serde(rename = "pub")]
        pubkey: String,
        sig: String,
    },
    /// Trickled ICE candidate (`candidate:` line as the browser emits it).
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMid", default)]
        sdp_mid: Option<String>,
    },
    /// The sender has no more candidates.
    EndOfCandidates,
    /// A measurement result (phone/web → daemon). Free-form JSON payload; the daemon appends
    /// it to results/*.jsonl untouched, adding `receivedAt`.
    Result { payload: serde_json::Value },
    /// Hang up.
    Bye,
    /// Daemon → viewer: I am alive, here is my peer id + public key (sent on join).
    Hello {
        #[serde(rename = "pub")]
        pubkey: String,
        version: String,
    },
    /// Rejected envelope (tamper test observability).
    Reject { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    pub v: u32,
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(flatten)]
    pub body: Body,
}

impl Envelope {
    pub fn new(from: &str, to: Option<&str>, session_id: &str, body: Body) -> Self {
        Self {
            v: VERSION,
            from: from.to_string(),
            to: to.map(str::to_string),
            session_id: session_id.to_string(),
            body,
        }
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("envelope serializes")
    }
    pub fn parse(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }
}

/// Pull `a=fingerprint:<hash> <HEX:..>` out of an SDP (session or first media section).
pub fn sdp_fingerprint(sdp: &str) -> Option<(String, String)> {
    sdp.lines().find_map(|l| {
        let l = l.trim_end_matches('\r');
        let rest = l.strip_prefix("a=fingerprint:")?;
        let mut it = rest.splitn(2, ' ');
        let hash = it.next()?.trim().to_ascii_lowercase();
        let hex = it.next()?.trim().to_ascii_uppercase();
        Some((hash, hex))
    })
}

/// Pull `a=ice-ufrag:` out of an SDP.
pub fn sdp_ufrag(sdp: &str) -> Option<String> {
    sdp.lines().find_map(|l| {
        l.trim_end_matches('\r')
            .strip_prefix("a=ice-ufrag:")
            .map(|s| s.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trips_offer() {
        let e = Envelope::new(
            "p1",
            Some("d1"),
            "vapp3-x",
            Body::Offer { sdp: "v=0".into(), pubkey: "k".into(), sig: "s".into() },
        );
        let j = e.to_json();
        assert!(j.contains("\"type\":\"offer\""));
        assert!(j.contains("\"pub\":\"k\""));
        assert_eq!(Envelope::parse(&j).unwrap(), e);
    }
    #[test]
    fn extracts_fingerprint_and_ufrag() {
        let sdp = "v=0\r\na=ice-ufrag:abcd\r\na=fingerprint:sha-256 AB:cd:EF\r\n";
        assert_eq!(sdp_fingerprint(sdp), Some(("sha-256".into(), "AB:CD:EF".into())));
        assert_eq!(sdp_ufrag(sdp), Some("abcd".into()));
    }
}
