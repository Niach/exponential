//! Typed `runConfigs.list` client (masterplan-v3 §7.3 — DB run configs,
//! EXP-2d). The router is a fresh server addition (lifted from the archive
//! branch); its pinned wire shape per §7.3.2:
//!
//! `runConfigs.list({projectId})` — **query**, member-read — →
//! `{configs: [{id, projectId, name, argv, cwd, env, sortOrder, createdAt,
//! updatedAt}]}` ordered by `sortOrder`, then `name`. (`workspaceId` is a
//! server-side denormalization detail and stays off the wire.)
//!
//! SECURITY (§7.3.5): these rows are **DB-stored argv the desktop spawns as
//! local child processes** — the one place synced/server data is ever
//! executed. The server's cwd/env sanitization is defense-in-depth only; the
//! mandatory compensating control is the client-side per-device Trust & Run
//! prompt keyed by a hash over the full fetched set. [`command_set_hash`]
//! provides that stable hash; the run-bar checks it against the per-device
//! trust store before every spawn and re-prompts on any change.
//!
//! Desktop-side spawning is argv-direct (`SpawnSpec`), **never a shell** —
//! `parseArgvLine` tokenization is a web-editor concern; by the time rows
//! reach this client `argv` is already the final vector.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// One `run_configs` row as the wire carries it. `env` is a `BTreeMap` so
/// iteration (and therefore [`command_set_hash`]) is deterministic.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
    pub id: String,
    pub project_id: String,
    pub name: String,
    /// Program + args, spawned as-is (no shell). Server-validated ≥ 1 item.
    pub argv: Vec<String>,
    /// Relative to the repo root; `None` = repo root (server rejects absolute
    /// paths and `..` segments — still re-checked desktop-side before spawn).
    #[serde(default)]
    pub cwd: Option<String>,
    /// Extra env (PATH/LD_PRELOAD/DYLD_* stripped server-side).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub sort_order: f64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
struct ListResponse {
    configs: Vec<RunConfig>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListInput<'a> {
    project_id: &'a str,
}

/// `runConfigs.list` — query, ordered by `sortOrder` then `name` server-side.
pub fn list(trpc: &TrpcClient, project_id: &str) -> Result<Vec<RunConfig>, ApiError> {
    let response: ListResponse =
        trpc.query_with_input("runConfigs.list", &ListInput { project_id })?;
    Ok(response.configs)
}

/// §7.3.5 `commandSetHash`: a stable hash over the *full fetched set* —
/// stable-serialize each `{name, argv, cwd, env}`, **sort by id**, hash.
/// Any content change (add / edit / reorder-that-alters-content / a config
/// from another author) yields a new hash, which un-trusts the project on
/// this device until the Trust & Run dialog confirms the new set.
///
/// The hash is SHA-256 (§7.3.5 allows blake3 or sha256; sha256 needs no new
/// dependency), hex-encoded. `sortOrder` and timestamps are deliberately NOT
/// hashed — pure reordering does not change what would execute.
pub fn command_set_hash(configs: &[RunConfig]) -> String {
    let mut sorted: Vec<&RunConfig> = configs.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    let mut canonical = String::new();
    for config in sorted {
        // serde_json on this struct is deterministic: struct fields serialize
        // in declaration order and BTreeMap keys in sorted order.
        #[derive(Serialize)]
        struct Canon<'a> {
            id: &'a str,
            name: &'a str,
            argv: &'a [String],
            cwd: &'a Option<String>,
            env: &'a BTreeMap<String, String>,
        }
        canonical.push_str(
            &serde_json::to_string(&Canon {
                id: &config.id,
                name: &config.name,
                argv: &config.argv,
                cwd: &config.cwd,
                env: &config.env,
            })
            .expect("canonical run-config serialization cannot fail"),
        );
        canonical.push('\n');
    }
    hex(&sha256(canonical.as_bytes()))
}

// ---- minimal SHA-256 (FIPS 180-4) — avoids a crypto dependency for a
// ---- change-detection hash. Not used for secrets.

fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for block in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    fn config(id: &str, name: &str, argv: &[&str]) -> RunConfig {
        RunConfig {
            id: id.to_string(),
            project_id: "proj-1".to_string(),
            name: name.to_string(),
            argv: argv.iter().map(|s| s.to_string()).collect(),
            cwd: None,
            env: BTreeMap::new(),
            sort_order: 0.0,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn list_decodes_configs_and_uses_get() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"configs":[
                {"id":"rc-1","projectId":"proj-1","name":"dev server",
                 "argv":["bun","run","dev"],"cwd":"apps/web","env":{"PORT":"5173"},
                 "sortOrder":1,"createdAt":"2026-07-03T00:00:00.000Z","updatedAt":"2026-07-03T00:00:00.000Z"}]}}}"#,
        );
        let configs = list(&client(&base), "proj-1").unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "dev server");
        assert_eq!(configs[0].argv, vec!["bun", "run", "dev"]);
        assert_eq!(configs[0].cwd.as_deref(), Some("apps/web"));
        assert_eq!(configs[0].env.get("PORT").map(String::as_str), Some("5173"));
        assert_eq!(configs[0].sort_order, 1.0);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/runConfigs.list?input="));
    }

    #[test]
    fn list_tolerates_null_cwd_and_empty_env() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"configs":[
                {"id":"rc-1","projectId":"proj-1","name":"test","argv":["cargo","test"],
                 "cwd":null,"env":{},"sortOrder":0}]}}}"#,
        );
        let configs = list(&client(&base), "proj-1").unwrap();
        assert_eq!(configs[0].cwd, None);
        assert!(configs[0].env.is_empty());
    }

    #[test]
    fn command_set_hash_is_order_independent() {
        // §7.3.5: sort by id before hashing — fetch order must not matter.
        let a = config("rc-a", "one", &["echo", "1"]);
        let b = config("rc-b", "two", &["echo", "2"]);
        assert_eq!(
            command_set_hash(&[a.clone(), b.clone()]),
            command_set_hash(&[b, a])
        );
    }

    #[test]
    fn command_set_hash_changes_on_any_content_change() {
        let base = vec![config("rc-a", "one", &["echo", "1"])];
        let baseline = command_set_hash(&base);

        let mut renamed = base.clone();
        renamed[0].name = "uno".to_string();
        assert_ne!(command_set_hash(&renamed), baseline);

        let mut new_argv = base.clone();
        new_argv[0].argv = vec!["sh".to_string(), "-c".to_string(), "evil".to_string()];
        assert_ne!(command_set_hash(&new_argv), baseline);

        let mut new_env = base.clone();
        new_env[0]
            .env
            .insert("NODE_OPTIONS".to_string(), "--require evil".to_string());
        assert_ne!(command_set_hash(&new_env), baseline);

        let mut added = base.clone();
        added.push(config("rc-b", "two", &["echo", "2"]));
        assert_ne!(command_set_hash(&added), baseline);
    }

    #[test]
    fn command_set_hash_ignores_pure_reorder_metadata() {
        // sortOrder / timestamps are display metadata — not what executes.
        let mut a = config("rc-a", "one", &["echo", "1"]);
        let baseline = command_set_hash(std::slice::from_ref(&a));
        a.sort_order = 42.0;
        a.updated_at = Some("2026-07-04T00:00:00.000Z".to_string());
        assert_eq!(command_set_hash(&[a]), baseline);
    }

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // > one 64-byte block
        assert_eq!(
            hex(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }
}
