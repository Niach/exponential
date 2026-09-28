# VAPP-3 manual matrix runbook (the part that needs a person, phones and other networks)

Everything below is prepared; the numbers land in `results/*.jsonl` by themselves (the apps send a
`result` envelope through the relay to the daemon, which appends it) and `bun spike/vapp-3/scripts/report.ts`
renders `RESULTS.md`. Steps marked **(you)** need Danny.

## 0. Pieces

| Piece | Local run (done by the spike) | Off-LAN run (cellular / office / hotel) |
|---|---|---|
| Signaling relay | docker dev relay `ws://192.168.178.71:4002`, secret `dev-steer-secret` | the staging relay (public, `wss://…/steer` on next.exponential.at) with tickets minted using ITS secret, or the dev relay exposed via a tunnel |
| TURN/STUN | native coturn on the Mac (`scripts/coturn-native.sh`, `192.168.178.71:3478`) | coturn on the Hetzner box (one-liner below); phones on cellular cannot reach the Mac's coturn |
| Daemon (the "desktop" side) | `peer-cli daemon` on this Mac, home Wi-Fi | same Mac, home Wi-Fi (that IS the scenario "home ↔ X") |
| Peers | browser on the Mac, iPhone Air on Wi-Fi, Android emulator | PeerHello on the iPhone / an Android phone on cellular or another Wi-Fi |

## 1. Public coturn on the Hetzner box **(you)**

```bash
# on the Hetzner host (docker installed). Replace PUBLIC_IP. Ports: 3478 udp+tcp, 49160-49200 udp.
mkdir -p /opt/vapp3-coturn && cat > /opt/vapp3-coturn/turnserver.conf <<'CONF'
listening-port=3478
listening-ip=0.0.0.0
external-ip=PUBLIC_IP
relay-ip=0.0.0.0
min-port=49160
max-port=49200
lt-cred-mech
user=exp:spike
realm=exponential.local
fingerprint
no-tls
no-dtls
no-cli
verbose
no-multicast-peers
CONF
docker run -d --name vapp3-coturn --restart unless-stopped --network host \
  -v /opt/vapp3-coturn/turnserver.conf:/etc/coturn/turnserver.conf:ro \
  coturn/coturn:4.7-alpine -c /etc/coturn/turnserver.conf
# firewall: allow 3478/udp, 3478/tcp, 49160-49200/udp. Remove afterwards: docker rm -f vapp3-coturn
```
Smoke test from the Mac: `peer-cli turn-probe --turn PUBLIC_IP:3478 --turn-user exp --turn-pass spike --turn-realm exponential.local`.

## 2. Relay reachable from cellular **(you, one of)**

- a) Staging relay: mint tickets with its secret: `bun spike/vapp-3/scripts/mint-tickets.ts --secret <STAGING_STEER_RELAY_SECRET> --relay wss://<staging relay host>/steer --session vapp3-cell --ttl 172800` (the secret is in Coolify; the relay accepts any signed sessionId; the room is scratch and never touches the DB). The daemon binary and the apps take the relay URL + ticket as inputs, nothing else changes.
- b) Or expose the dev relay: any tunnel to `localhost:4002` (Tailscale funnel, cloudflared) and use that wss URL with `dev-steer-secret` tickets.

## 3. Start the daemon on the Mac (home Wi-Fi)

```bash
cd ~/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-3
export CARGO_TARGET_DIR=$PWD/spike/vapp-3/rust/target
cargo run --release --manifest-path spike/vapp-3/rust/Cargo.toml -p peer-cli -- daemon \
  --relay <RELAY_WS_URL> --ticket <PUBLISHER_TICKET> --session vapp3-cell --peer-id daemon \
  --stun PUBLIC_IP:3478 --turn PUBLIC_IP:3478 --turn-user exp --turn-pass spike --turn-realm exponential.local \
  --policy all --out spike/vapp-3/results/matrix-daemon.jsonl
```

## 4. Phones **(you)**

- iPhone Air: install PeerHello (steps in `ios/README.md`; Xcode run from this worktree, automatic signing with team V6W7BVCSM8).
- Android phone (if you have one): `adb install` the PeerHello APK (`android/README.md`).
- In the app: Relay URL + Viewer ticket + Session `vapp3-cell`, TURN `PUBLIC_IP:3478`, Policy **all**, Scenario = the row you are testing. Then: **Connect → Bench → Send results**. Repeat 5× per row. For the reconnect rows: after Bench, background the app for 30 s and return (the app reconnects and records `background`), and toggle Wi-Fi off/on while connected (records `network-switch`), then **Send results** again.

## 5. The matrix

| Row (scenario value) | Peer | Where | Runs |
|---|---|---|---|
| `same-lan` | iPhone + Android on home Wi-Fi | done locally where possible | 5 each |
| `home-wifi-cellular` | phone on cellular, carrier 1 | anywhere with signal | 5 |
| `home-wifi-cellular` (carrier 2) | second SIM/phone, note the carrier in the app's Notes field | | 5 |
| `home-office-wifi` | phone on office/hotel Wi-Fi | office / hotel | 5 |
| `cellular-cellular` | needs the DAEMON on cellular too: run `peer-cli daemon` on a laptop tethered to a phone hotspot, peer on another carrier | | 5 |

Then `bun spike/vapp-3/scripts/report.ts` (from the repo root) and read `RESULTS.md`. The direct-success column of each row = the share of connections that did NOT need TURN (sizes #VAPP-38).
