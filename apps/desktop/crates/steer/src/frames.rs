//! The frozen wire protocol's Rust mirror (masterplan-v3 §8.1) —
//! byte-for-byte against `apps/steer-relay/src/protocol.ts`. Plain serde, no
//! gpui, no tokio: unit-testable against hand-built vectors.
//!
//! Field-name discipline (each was a live native bug or a protocol subtlety):
//! the relay JSON is **camelCase** (`deviceId`, `sessionId`, `issueId`,
//! `steererId`, `userId`, `deviceLabel`); the tag values are **snake_case**
//! (`online`, `hello`, `start_session`, …); the input field is **`data`**
//! (UTF-8 string, ≤ 8 KiB), never `bytes`. The relay zod-validates every text
//! frame and silently drops non-conforming ones (`parseClientFrame` returns
//! `null` ⇒ ignored) — a typo is a silent hang, not an error.
//!
//! Terminal output is NEVER JSON: it is a binary WebSocket frame whose first
//! byte is [`OUTPUT_OPCODE`] followed by verbatim PTY bytes. The desktop is a
//! publisher, so it **produces** `0x01` frames and never consumes them.

use serde::{Deserialize, Serialize};

/// First byte of every binary terminal-output frame (protocol.ts
/// `OUTPUT_OPCODE`).
pub const OUTPUT_OPCODE: u8 = 0x01;

/// Close codes (protocol.ts). Handle each distinctly (§8.6).
pub const CLOSE_SESSION_ENDED: u16 = 4001;
pub const CLOSE_REPLACED: u16 = 4002;
pub const CLOSE_UNAUTHORIZED: u16 = 4003;
pub const CLOSE_SLOW_CONSUMER: u16 = 4008;

/// Frame a raw PTY chunk as a `0x01` binary output frame.
pub fn output_frame(chunk: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(chunk.len() + 1);
    buf.push(OUTPUT_OPCODE);
    buf.extend_from_slice(chunk);
    buf
}

/// `view` | `steer` — mirrors `packages/steer-ticket` `SteerPerm`.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SteerPerm {
    View,
    Steer,
}

/// `control` | `publisher` | `viewer` — mirrors `SteerRole`.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SteerRole {
    Control,
    Publisher,
    Viewer,
}

// ── Client → relay (TEXT frames, JSON `{t, …}`) ─────────────────────────────

/// Every frame this publisher may send. Serialize-only (the relay never
/// echoes client frames back).
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientFrame<'a> {
    #[serde(rename_all = "camelCase")]
    Online {
        device_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        device_label: Option<&'a str>,
    },
    #[serde(rename_all = "camelCase")]
    Hello {
        session_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        issue_id: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cols: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        rows: Option<u16>,
    },
    Join,
    Resize {
        cols: u16,
        rows: u16,
    },
    /// NOTE: the field is `data` — a UTF-8 `String`, relay-enforced ≤ 8 KiB —
    /// NOT `bytes`. (A native client shipped `bytes` and steer input silently
    /// no-op'd.)
    Input {
        data: String,
    },
    Claim,
    Release,
    Kill,
    Bye {
        #[serde(skip_serializing_if = "Option::is_none")]
        outcome: Option<&'a str>,
    },
    /// One PUBLIC activity event (§P7 live-coding view). Serializes to
    /// `{"t":"activity","event":{...}}`; the relay fans it to `public_viewer`
    /// sockets only. The event text is ALREADY redacted by the emitter.
    Activity {
        event: ActivityEvent,
    },
}

/// A single public activity event (masterplan §P7) — the desktop emits these
/// from the Claude session transcript + worktree diffs, already redacted. Wire
/// mirror of `apps/steer-relay/src/protocol.ts` `activityEventSchema`
/// (discriminated on `kind`). Serialize-only.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActivityEvent {
    /// Assistant prose (a `text` content block).
    Narration { text: String },
    /// A tool-call headline: the tool name + a single primary argument
    /// (file path / pattern / Bash description — NEVER a command string or a
    /// tool result).
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// A worktree unified diff snapshot (latest replaces prior, viewer-side).
    Diff { diff: String },
}

impl ClientFrame<'_> {
    /// The JSON text-frame body. Serialization of this enum cannot fail
    /// (no non-string map keys, no non-finite floats).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("ClientFrame serialization cannot fail")
    }
}

// ── Relay → client (TEXT frames) ────────────────────────────────────────────

/// One presence entry (protocol.ts `PresenceViewer`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PresenceViewer {
    pub user_id: String,
    pub name: String,
    pub perm: SteerPerm,
}

/// Every frame the relay may send. Deserialize-only.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerFrame {
    #[serde(rename_all = "camelCase")]
    Presence {
        viewers: Vec<PresenceViewer>,
        /// `steererId` is `string | null` on the wire.
        steerer_id: Option<String>,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    #[serde(rename_all = "camelCase")]
    StartSession {
        issue_id: String,
    },
    /// Steerer keystrokes, relay → publisher.
    Input {
        data: String,
    },
    Resync,
    Kill,
    Bye {
        #[serde(default)]
        outcome: Option<String>,
    },
    Error {
        code: String,
        #[serde(default)]
        message: Option<String>,
    },
}

impl ServerFrame {
    /// Parse a relay text frame; `None` for anything non-conforming (mirror
    /// of the relay's own silent-drop posture — an unknown future frame must
    /// not kill the socket).
    pub fn parse(raw: &str) -> Option<ServerFrame> {
        serde_json::from_str(raw).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ClientFrame vectors — authored from protocol.ts zod schemas + the
    // hub tests' literal frames (`hub.test.ts` sends exactly these shapes).

    #[test]
    fn online_serializes_camel_case_device_fields() {
        assert_eq!(
            ClientFrame::Online {
                device_id: "dev-1",
                device_label: Some("MacBook"),
            }
            .to_json(),
            r#"{"t":"online","deviceId":"dev-1","deviceLabel":"MacBook"}"#
        );
        assert_eq!(
            ClientFrame::Online {
                device_id: "dev-1",
                device_label: None,
            }
            .to_json(),
            r#"{"t":"online","deviceId":"dev-1"}"#
        );
    }

    #[test]
    fn hello_serializes_session_and_geometry() {
        // hub.test.ts connectPublisher sends exactly this frame (120×40).
        assert_eq!(
            ClientFrame::Hello {
                session_id: "sess-1",
                issue_id: Some("issue-1"),
                cols: Some(120),
                rows: Some(40),
            }
            .to_json(),
            r#"{"t":"hello","sessionId":"sess-1","issueId":"issue-1","cols":120,"rows":40}"#
        );
        assert_eq!(
            ClientFrame::Hello {
                session_id: "sess-1",
                issue_id: None,
                cols: None,
                rows: None,
            }
            .to_json(),
            r#"{"t":"hello","sessionId":"sess-1"}"#
        );
    }

    #[test]
    fn input_field_is_data_not_bytes() {
        // §8.1: the exact regression vector from the spec.
        assert_eq!(
            ClientFrame::Input { data: "x".into() }.to_json(),
            r#"{"t":"input","data":"x"}"#
        );
    }

    #[test]
    fn bare_frames_serialize_tag_only() {
        assert_eq!(ClientFrame::Join.to_json(), r#"{"t":"join"}"#);
        assert_eq!(ClientFrame::Claim.to_json(), r#"{"t":"claim"}"#);
        assert_eq!(ClientFrame::Release.to_json(), r#"{"t":"release"}"#);
        assert_eq!(ClientFrame::Kill.to_json(), r#"{"t":"kill"}"#);
        assert_eq!(ClientFrame::Bye { outcome: None }.to_json(), r#"{"t":"bye"}"#);
        assert_eq!(
            ClientFrame::Bye {
                outcome: Some("exit:0")
            }
            .to_json(),
            r#"{"t":"bye","outcome":"exit:0"}"#
        );
        assert_eq!(
            ClientFrame::Resize { cols: 132, rows: 43 }.to_json(),
            r#"{"t":"resize","cols":132,"rows":43}"#
        );
    }

    #[test]
    fn activity_frame_serializes_to_the_relay_schema() {
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Narration {
                    text: "Reading the file".into()
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"narration","text":"Reading the file"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Tool {
                    name: "Edit".into(),
                    detail: Some("src/main.rs".into())
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"tool","name":"Edit","detail":"src/main.rs"}}"#
        );
        // detail is omitted when absent.
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Tool {
                    name: "TodoWrite".into(),
                    detail: None
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"tool","name":"TodoWrite"}}"#
        );
        assert_eq!(
            ClientFrame::Activity {
                event: ActivityEvent::Diff {
                    diff: "--- a\n+++ b\n".into()
                }
            }
            .to_json(),
            r#"{"t":"activity","event":{"kind":"diff","diff":"--- a\n+++ b\n"}}"#
        );
    }

    #[test]
    fn client_frames_satisfy_relay_zod_constraints() {
        // Round-trip our own serialization through a permissive parse to
        // assert the tag names the relay's discriminated union expects.
        for (frame, tag) in [
            (
                ClientFrame::Online {
                    device_id: "d",
                    device_label: None,
                },
                "online",
            ),
            (
                ClientFrame::Hello {
                    session_id: "s",
                    issue_id: None,
                    cols: None,
                    rows: None,
                },
                "hello",
            ),
            (ClientFrame::Join, "join"),
            (ClientFrame::Resize { cols: 1, rows: 1 }, "resize"),
            (ClientFrame::Input { data: String::new() }, "input"),
            (ClientFrame::Claim, "claim"),
            (ClientFrame::Release, "release"),
            (ClientFrame::Kill, "kill"),
            (ClientFrame::Bye { outcome: None }, "bye"),
        ] {
            let value: serde_json::Value = serde_json::from_str(&frame.to_json()).unwrap();
            assert_eq!(value["t"], tag, "tag mismatch for {frame:?}");
        }
    }

    // ── ServerFrame vectors — captured relay strings (hub.ts `frame(...)`
    // emits `JSON.stringify` of exactly these objects).

    #[test]
    fn presence_deserializes_viewers_and_nullable_steerer() {
        // hub.ts broadcastPresence: viewers from room.viewers.values(),
        // steererId: room.steerer?.claims.sub ?? null.
        let frame = ServerFrame::parse(
            r#"{"t":"presence","viewers":[{"userId":"viewer-1","name":"Dennis","perm":"steer"}],"steererId":"viewer-1"}"#,
        )
        .unwrap();
        assert_eq!(
            frame,
            ServerFrame::Presence {
                viewers: vec![PresenceViewer {
                    user_id: "viewer-1".into(),
                    name: "Dennis".into(),
                    perm: SteerPerm::Steer,
                }],
                steerer_id: Some("viewer-1".into()),
            }
        );
        let frame =
            ServerFrame::parse(r#"{"t":"presence","viewers":[],"steererId":null}"#).unwrap();
        assert_eq!(
            frame,
            ServerFrame::Presence {
                viewers: vec![],
                steerer_id: None,
            }
        );
    }

    #[test]
    fn start_session_deserializes_camel_issue_id() {
        // hub.ts startSession: frame({ t: `start_session`, issueId }).
        assert_eq!(
            ServerFrame::parse(r#"{"t":"start_session","issueId":"issue-9"}"#).unwrap(),
            ServerFrame::StartSession {
                issue_id: "issue-9".into()
            }
        );
    }

    #[test]
    fn remaining_server_frames_deserialize() {
        assert_eq!(
            ServerFrame::parse(r#"{"t":"resize","cols":120,"rows":40}"#).unwrap(),
            ServerFrame::Resize { cols: 120, rows: 40 }
        );
        assert_eq!(
            ServerFrame::parse(r#"{"t":"input","data":"ls\r"}"#).unwrap(),
            ServerFrame::Input { data: "ls\r".into() }
        );
        assert_eq!(ServerFrame::parse(r#"{"t":"resync"}"#).unwrap(), ServerFrame::Resync);
        assert_eq!(ServerFrame::parse(r#"{"t":"kill"}"#).unwrap(), ServerFrame::Kill);
        assert_eq!(
            ServerFrame::parse(r#"{"t":"bye","outcome":"publisher_lost"}"#).unwrap(),
            ServerFrame::Bye {
                outcome: Some("publisher_lost".into())
            }
        );
        assert_eq!(
            ServerFrame::parse(r#"{"t":"bye"}"#).unwrap(),
            ServerFrame::Bye { outcome: None }
        );
        assert_eq!(
            ServerFrame::parse(r#"{"t":"error","code":"no_such_session"}"#).unwrap(),
            ServerFrame::Error {
                code: "no_such_session".into(),
                message: None,
            }
        );
    }

    #[test]
    fn unknown_or_malformed_frames_parse_to_none() {
        // Mirror of the relay's silent-drop: never kill the socket on a
        // future frame type or junk.
        assert_eq!(ServerFrame::parse(r#"{"t":"future_frame"}"#), None);
        assert_eq!(ServerFrame::parse("not json"), None);
        assert_eq!(ServerFrame::parse(r#"{"cols":1}"#), None);
    }

    #[test]
    fn output_frame_prefixes_opcode() {
        assert_eq!(output_frame(b"hi"), vec![0x01, b'h', b'i']);
        assert_eq!(output_frame(b""), vec![0x01]);
    }
}
