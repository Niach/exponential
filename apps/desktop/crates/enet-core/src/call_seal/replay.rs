//! The replay of voice-call.json `seal`: every vector recomputed from its
//! inputs, every entry of `refused` refused with its reason. It lives in the
//! core (not in a test) because every key hangs on the eph secrets and the
//! fixed-eph constructor never leaves this crate: the phones replay the
//! fixture by calling [`selftest`] through the binding.

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::*;

fn unhex(v: &Value) -> Vec<u8> {
    data_encoding::HEXLOWER.decode(v.as_str().unwrap_or("").as_bytes()).unwrap_or_default()
}
fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn count(v: &Value) -> usize {
    v.as_u64().unwrap_or(0) as usize
}
fn list(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}
fn same<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) -> std::result::Result<(), String> {
    if got == want { Ok(()) } else { Err(format!("{what}: got {got:?}, want {want:?}")) }
}
fn reason<T>(got: Result<T>) -> &'static str {
    got.map_or_else(No::as_str, |_| "accepted")
}

struct Ends {
    caller_hello: [u8; CALLER_HELLO_BYTES],
    host_hello: [u8; HOST_HELLO_BYTES],
    caller: CallSeal,
    host: CallSeal,
}

struct Replay<'a> {
    seal: &'a Value,
}

impl Replay<'_> {
    fn pad(&self, step: Option<(&str, usize)>) -> CallSealPad {
        let of = |direction: &str| step.filter(|(d, _)| *d == direction).map_or(count(&self.seal["audioPadStep"][direction]), |(_, step)| step);
        CallSealPad { audio_up: of("up"), audio_down: of("down"), control_pad: count(&self.seal["controlPadBytes"]), max_plain: count(&self.seal["maxPlainBytes"]) }
    }

    fn handshake(&self, role: CallRole, with: &Value) -> Result<CallHandshake> {
        let field = |name: &str| if with[name].is_null() { &self.seal["inputs"][name] } else { &with[name] };
        let eph = unhex(field(if role == CallRole::Caller { "callerEphSecret" } else { "hostEphSecret" }));
        CallHandshake::with_eph_secret(role, Some(text(field("rootId"))), count(field("epoch")) as u32, eph.try_into().map_err(|_| No::Input)?)
    }

    /// `finish` of one end with the fixture's inputs, `with` over them.
    fn finish(&self, handshake: &CallHandshake, peer_hello: &[u8], with: &Value, pad: CallSealPad) -> Result<CallSeal> {
        let field = |name: &str| if with[name].is_null() { &self.seal["inputs"][name] } else { &with[name] };
        handshake.finish(&CallFinish {
            secret: &unhex(field("secret")),
            call_id: text(field("callId")),
            agent_id: text(field("agentId")),
            device_id: text(field("deviceId")),
            peer_hello,
            root_id: Some(text(field("rootId"))),
            epoch: count(field("epoch")) as u32,
            pad,
        })
    }

    /// A fresh pair of ends. The host finishes first (its hello carries its
    /// tag); `after` = the up frames the HOST opens first (the caller's proof).
    fn ends(&self, with: &Value, step: Option<(&str, usize)>, after: &Value) -> std::result::Result<Ends, String> {
        let made = |err: No| format!("the ends: {err}");
        let (caller, host) = (self.handshake(CallRole::Caller, with).map_err(made)?, self.handshake(CallRole::Host, with).map_err(made)?);
        let caller_hello = caller.hello().map_err(made)?;
        let host = self.finish(&host, &caller_hello, with, self.pad(step)).map_err(made)?;
        let host_hello = host.host_hello().ok_or("the host's finish gave no hello")?;
        let caller = self.finish(&caller, &host_hello, with, self.pad(step)).map_err(made)?;
        for frame in list(after) {
            host.open_audio(&unhex(frame)).map_err(|err| format!("after: {err}"))?;
        }
        Ok(Ends { caller_hello, host_hello, caller, host })
    }

    fn opened(end: &CallSeal, frame: &Value) -> Value {
        match end.open_audio(&unhex(frame)) {
            Ok(o) => serde_json::json!({ "seq": o.seq, "flags": o.flags, "packet": hex(&o.packet), "missing": o.missing }),
            Err(err) => Value::String(err.as_str().into()),
        }
    }

    fn constants(&self, kinds: &Value) -> std::result::Result<(), String> {
        let s = self.seal;
        same("version", s["version"].as_u64(), Some(CALL_SEAL_VERSION.into()))?;
        same("cap", text(&s["cap"]), CALL_SEAL_CAP)?;
        same("domain", text(&s["domain"]).as_bytes(), DOMAIN)?;
        same("labels", ["upAudio", "downAudio", "downControl", "downHello"].map(|name| text(&s["labels"][name])), LABELS)?;
        same("labels", s["labels"].as_object().map(|labels| labels.len()), Some(LABELS.len()))?;
        same("roles", [&s["roles"]["caller"], &s["roles"]["host"]].map(count), [CallRole::Caller, CallRole::Host].map(|role| role.byte() as usize))?;
        same("helloBytes", [&s["helloBytes"]["caller"], &s["helloBytes"]["host"]].map(count), [CALLER_HELLO_BYTES, HOST_HELLO_BYTES])?;
        same("frameTypes", [&s["frameTypes"]["audio"], &s["frameTypes"]["hello"]].map(count), [TYPE_AUDIO, TYPE_HELLO].map(usize::from))?;
        same("kinds", [&kinds["up"], &kinds["down"]].map(count), [KIND_UP, KIND_DOWN].map(usize::from))?;
        same("the default pad", self.pad(None), CallSealPad::default())
    }

    fn derived(&self) -> std::result::Result<(), String> {
        let (d, i) = (&self.seal["derived"], &self.seal["inputs"]);
        let pair = self.ends(&Value::Null, None, &Value::Null)?;
        same("callerHello", b64url_encode(&pair.caller_hello).as_str(), text(&d["callerHello"]))?;
        same("hostHello", host_hello_text(&pair.host_hello).as_str(), text(&d["hostHello"]))?;
        same("hostHelloTag", hex(&pair.host_hello[CALLER_HELLO_BYTES..]).as_str(), text(&d["hostHelloTag"]))?;
        same("callerHelloFrame", hex(&hello_frame(&pair.caller_hello)).as_str(), text(&d["callerHelloFrame"]))?;
        same("the hello of its frame", hello_of_frame(&unhex(&d["callerHelloFrame"])), Some(&pair.caller_hello[..]))?;
        same("the hello of its text", host_hello_of_text(text(&d["hostHello"])), Some(pair.host_hello.to_vec()))?;
        same("a caller hello is no host hello", host_hello_of_text(text(&d["callerHello"])), None)?;
        same("eph", [parse_hello(&pair.caller_hello, CallRole::Caller), parse_hello(&pair.host_hello, CallRole::Host)].map(|h| h.map(|h| hex(&h.eph)).unwrap_or_default()), [&d["callerEph"], &d["hostEph"]].map(|v| text(v).to_string()))?;
        let f = CallFinish { secret: &unhex(&i["secret"]), call_id: text(&i["callId"]), agent_id: text(&i["agentId"]), device_id: text(&i["deviceId"]), peer_hello: &[], root_id: None, epoch: 0, pad: self.pad(None) };
        for (own, peer) in [("callerEphSecret", "hostEph"), ("hostEphSecret", "callerEph")] {
            let key = |v: &Value| <[u8; 32]>::try_from(unhex(v)).map_err(|_| format!("{own}: not 32 bytes"));
            let end = derive(&key(&i[own])?, &key(&d[peer])?, &f, &pair.caller_hello, &pair.host_hello).map_err(|err| format!("{own}: {err}"))?;
            same("dh", hex(&end.dh).as_str(), text(&d["dh"]))?;
            same("the transcript's length", end.transcript.len(), 188)?;
            same("transcriptSha256", hex(&Sha256::digest(&end.transcript)).as_str(), text(&d["transcriptSha256"]))?;
            same("keys", end.keys.map(|key| hex(&key)), ["upAudio", "downAudio", "downControl", "downHello"].map(|name| text(&d["keys"][name]).to_string()))?;
        }
        Ok(())
    }

    fn vector(&self, v: &Value) -> std::result::Result<(), String> {
        let direction = text(&v["direction"]);
        let pair = |step: Option<(&str, usize)>| self.ends(&Value::Null, step, &v["after"]);
        match text(&v["op"]) {
            "audio" => {
                let pair = pair(Some((direction, count(&v["step"]))))?;
                let (sealer, opener) = if direction == "up" { (&pair.caller, &pair.host) } else { (&pair.host, &pair.caller) };
                // The sealer owns its counter: `ctr` is reached by sealing, never set.
                for _ in 0..count(&v["ctr"]) {
                    sealer.seal_audio(0, 0, &[]).map_err(|err| format!("reaching the ctr: {err}"))?;
                }
                let frame = sealer.seal_audio(count(&v["seq"]) as u16, count(&v["flags"]) as u8, &unhex(&v["packet"])).map_err(|err| err.to_string())?;
                same("frame", hex(&frame).as_str(), text(&v["frame"]))?;
                same("aad", hex(&frame[..HEADER_BYTES]).as_str(), text(&v["aad"]))?;
                same("the frame's length", frame.len(), HEADER_BYTES + INNER_BYTES + TAG_BYTES + count(&v["step"]))?;
                if !v["frameSha256"].is_null() {
                    same("frameSha256", hex(&Sha256::digest(&frame)).as_str(), text(&v["frameSha256"]))?;
                }
                same("opened", Self::opened(opener, &v["frame"]), v["opened"].clone())
            }
            "hello" => {
                let pair = pair(None)?;
                same("hostHello", host_hello_text(&pair.host_hello).as_str(), text(&v["hostHello"]))?;
                same("aad", hex(&pair.host_hello[..CALLER_HELLO_BYTES]).as_str(), text(&v["aad"]))?;
                same("tag", hex(&pair.host_hello[CALLER_HELLO_BYTES..]).as_str(), text(&v["tag"]))?;
                let hello = host_hello_of_text(text(&v["hostHello"])).ok_or("hostHello is no host hello")?;
                let caller = self.handshake(CallRole::Caller, &Value::Null).map_err(|err| err.to_string())?;
                same("the caller's finish opens it", reason(self.finish(&caller, &hello, &Value::Null, self.pad(None))), "accepted")
            }
            "control" => {
                let pair = pair(None)?;
                let data = pair.host.seal_control(text(&v["text"])).map_err(|err| err.to_string())?;
                same("data", data.as_str(), text(&v["data"]))?;
                let raw = b64url_decode(&data).unwrap_or_default();
                same("aad", hex(&raw[..4]).as_str(), text(&v["aad"]))?;
                same("the frame's length", raw.len(), 4 + count(&v["plainBytes"]) + TAG_BYTES)?;
                let plain = pair.caller.open_control(text(&v["data"])).map_err(|err| err.to_string())?;
                same("the opened text", plain.as_str(), format!("{:<1$}", text(&v["text"]), count(&v["plainBytes"])).as_str())?;
                // Every control parser in use takes the trailing spaces.
                same("the text as JSON", serde_json::from_str::<Value>(&plain).ok(), serde_json::from_str(text(&v["text"])).ok())
            }
            "open" => {
                let pair = pair(None)?;
                same("opened", Self::opened(if direction == "up" { &pair.host } else { &pair.caller }, &v["frame"]), v["opened"].clone())
            }
            "keys" => {
                let lower = Value::Object(v["with"].as_object().into_iter().flatten().map(|(key, value)| (key.clone(), text(value).to_lowercase().into())).collect());
                for with in [&v["with"], &lower] {
                    let pair = self.ends(with, None, &Value::Null)?;
                    same("callerHello", b64url_encode(&pair.caller_hello).as_str(), text(&v["callerHello"]))?;
                    same("hostHello", host_hello_text(&pair.host_hello).as_str(), text(&v["hostHello"]))?;
                    for end in [&pair.caller, &pair.host] {
                        same("keys", [0, 1, 2].map(|at| hex(&end.keys[at])), ["upAudio", "downAudio", "downControl"].map(|name| text(&v["keys"][name]).to_string()))?;
                    }
                }
                Ok(())
            }
            other => Err(format!("unknown vector op {other}")),
        }
    }

    fn refused(&self, r: &Value) -> std::result::Result<(), String> {
        let (direction, want) = (text(&r["direction"]), text(&r["reason"]));
        let pair = |step: Option<(&str, usize)>| self.ends(&Value::Null, step, &r["after"]);
        match text(&r["op"]) {
            "openAudio" => {
                let pair = pair(None)?;
                let opener = if direction == "up" { &pair.host } else { &pair.caller };
                let mark = || opener.recv.lock().unwrap().0;
                let before = mark();
                same("reason", reason(opener.open_audio(&unhex(&r["frame"]))), want)?;
                // A refused frame moves no mark: the next good one still opens.
                same("the mark", mark(), before)
            }
            "openControl" => {
                let pair = pair(None)?;
                for data in list(&r["afterData"]) {
                    pair.caller.open_control(text(data)).map_err(|err| format!("afterData: {err}"))?;
                }
                same("reason", reason(pair.caller.open_control(text(&r["data"]))), want)
            }
            "sealAudio" => {
                let pair = pair(Some((direction, count(&r["step"]))))?;
                let sealer = if direction == "up" { &pair.caller } else { &pair.host };
                if !r["packetBytes"].is_null() {
                    same("reason", reason(sealer.seal_audio(0, 0, &vec![0; count(&r["packetBytes"])])), want)?;
                    // What was refused took no counter.
                    return same("the counter", sealer.send.lock().unwrap().0, 0);
                }
                // No end can set a counter; only this replay puts one at the last value.
                sealer.send.lock().unwrap().0 = r["ctr"].as_u64().unwrap_or(0);
                let (seq, flags, packet) = (count(&r["seq"]) as u16, count(&r["flags"]) as u8, unhex(&r["packet"]));
                same("the last frame", sealer.seal_audio(seq, flags, &packet).map(|frame| hex(&frame)).unwrap_or_default().as_str(), text(&r["frame"]))?;
                same("reason", reason(sealer.seal_audio(seq, flags, &packet)), want)?;
                same("reason, an empty frame", reason(sealer.seal_audio(0, 0, &[])), want)
            }
            "sealControl" => {
                let pair = pair(None)?;
                same("reason", reason(pair.host.seal_control(&"x".repeat(count(&r["textBytes"])))), want)?;
                same("the counter", pair.host.send.lock().unwrap().1, 0)?;
                if r["after"].is_null() {
                    return Ok(());
                }
                let max = count(&self.seal["maxPlainBytes"]);
                let sealed = pair.host.seal_control(&"x".repeat(max)).map_err(|err| format!("a text of maxPlainBytes: {err}"))?;
                same("a text of maxPlainBytes", b64url_decode(&sealed).map(|raw| raw.len()), Some(4 + max + TAG_BYTES))
            }
            "finish" => {
                let role = if text(&r["role"]) == "host" { CallRole::Host } else { CallRole::Caller };
                let end = self.handshake(role, &Value::Null).map_err(|err| err.to_string())?;
                let finish = |hello: &Value, with: &Value| reason(self.finish(&end, &b64url_decode(text(hello)).unwrap_or_default(), with, self.pad(None)));
                if !r["first"].is_null() {
                    finish(&r["first"], &Value::Null);
                }
                same("reason", finish(&r["peerHello"], r), want)?;
                // `finish` works once whatever it answered: there is no second try.
                same("a finish afterwards", finish(&self.seal["derived"]["hostHello"], &Value::Null), "finished")
            }
            other => Err(format!("unknown refused op {other}")),
        }
    }
}

/// Replays `seal` of voice-call.json (the whole fixture's JSON) and names
/// every vector and refusal that did not hold. Stateless; no key leaves it.
pub fn selftest(fixture_json: &str) -> std::result::Result<(), String> {
    let fixture: Value = serde_json::from_str(fixture_json).map_err(|err| format!("the fixture is not JSON: {err}"))?;
    let replay = Replay { seal: &fixture["seal"] };
    let (vectors, refused) = (list(&replay.seal["vectors"]), list(&replay.seal["refused"]));
    if vectors.is_empty() || refused.is_empty() {
        return Err("the fixture has no seal.vectors or no seal.refused".into());
    }
    let named = |name: &str, result: std::result::Result<(), String>| result.err().map(|err| format!("{name}: {err}"));
    let failed: Vec<String> = [named("constants", replay.constants(&fixture["wire"]["kinds"])), named("derived", replay.derived())]
        .into_iter()
        .chain(vectors.iter().map(|v| named(text(&v["name"]), replay.vector(v))))
        .chain(refused.iter().map(|r| named(text(&r["name"]), replay.refused(r))))
        .flatten()
        .collect();
    if failed.is_empty() { Ok(()) } else { Err(failed.join("\n")) }
}
