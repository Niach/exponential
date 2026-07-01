# Exponential — Vision

**The Linear-simple issue tracker that actually fixes the issue.**

---

## The one-sentence pitch

Exponential is the ultimate Linear alternative — simpler and cheaper — that is also a full coding IDE with embedded AI agents, so issues don't just get tracked, they get **fixed**: clone → AI → PR, driven from anywhere, even your phone.

---

## What Exponential is

Exponential is an issue tracker with the simplicity of Linear — and simplicity *is* the moat, not a limitation we apologize for. You get a fast, real-time tracker: issues, projects, labels, priorities, comments, My Issues, an inbox. Nothing you have to learn. Nothing you have to configure. It syncs live across web, iOS, Android, macOS, and Linux.

But Exponential has a superpower Linear will never have: **the fix flow is built in.** Every issue can become a coding task. Point an AI agent at it, and it clones the repo into a git worktree, does the work, and opens a pull request. One issue = one PR = one worktree. The desktop app is a real IDE — an embedded terminal, JetBrains-style run configs with a play button, concurrent agent sessions, multi-window, and syntax-highlighted PR diff review. The web and mobile apps are the coordination and *remote-control* surface: you can create and triage issues, review diffs, and — the part nobody else does — **watch a live agent terminal running on your desktop and type into it, from your phone.**

Because the fix flow lives at the center, Exponential is **coding-first**. Repositories are a first-class workspace entity. GitHub is effectively mandatory: if an issue has no linked repo, the agent can't fix it and honestly routes it to a human instead of pretending. That honesty runs through the whole product.

And when work reaches an end-user — a bug filed through the embedded feedback widget — Exponential closes the loop with a **built-in one-way helpdesk**: the reporter becomes a subscriber and gets an automatic resolution email the moment their issue is done. Support, without buying a support tool.

Everything works fully **self-hosted**. Every feature. The only thing that degrades on self-hosted is billing — and it degrades to unlimited.

---

## Platform roles

The split is locked and load-bearing. The desktop is where code runs. The web and mobile clients coordinate and remote-control — they never host a shell or bundle a CLI.

| Surface | Role | Has |
| --- | --- | --- |
| **Desktop** — macOS (SwiftUI, glass) · Linux (Zig/GTK4, web pixel-parity) | The **IDE**. Where agents and builds actually run. | Embedded libghostty terminal, JetBrains-style run configs + play button, concurrent agent sessions, multi-window (detached terminal/diff/preview), syntax-highlighted side-by-side diff review |
| **Web** + **Mobile** — iOS (native glass) · Android (Compose) | The **coordination + remote-control** surface. | Create/triage/assign, comments, My Issues, Inbox, review the same diff, **watch + steer a live desktop agent session** — no local terminal, no local agent runtime, no bundled CLI |

The desktop runs the interactive agent inside libghostty; the shared Rust `agent-core` runs the loop headless. Web and mobile stay pure coordination surfaces on purpose.

---

## The killer flow

An issue arrives while you're out. Your desktop — agent-capable — is running at home.

1. The issue **emails your phone** (email is a first-class delivery channel, free on every tier). You open it in the mobile app.
2. From your phone, you **assign it to your desktop agent**. The agent resolves the clone target from the workspace repository registry, spins up a git worktree at home, and starts working: clone → AI → PR.
3. You **watch the live terminal** stream from your desktop, on your phone — and when the agent asks a question or heads down the wrong path, you **type into it**. Full bidirectional steering, not read-only status. Electric syncs the record; the outbound relay carries the bytes.
4. The agent opens a PR. You **review the syntax-highlighted side-by-side diff** on the same phone.
5. It merges. You never sat down at a computer.

That's the moat in one story: an issue went from *reported while you were away* to *merged PR* — steered from your pocket. Linear can track the issue. Exponential fixes it.

---

## Core principles

1. **Simpler than Linear is the moat.** The features we *refuse* to build are the product's competitive edge. Every "no" keeps the tool learnable in minutes and defensible for years.
2. **One great flow, not a hundred features.** The clone → AI → PR loop is a frozen, proven runtime we extend and never rewrite. We polish the one path that matters instead of scattering shallow features.
3. **Coding-first / GitHub-mandatory.** Repositories are first-class; an unlinked issue deterministically routes to a human. We'd rather be honest about what the agent can't fix than fake it.
4. **Nothing gets lost — and we never monetize on that.** In-app + push + email fan-out is table-stakes and **free on every tier**. We monetize agents, seats, repos, and workspace tier — never anxiety.
5. **Self-hosted-first parity.** Every feature works self-hosted at full fidelity. The relay works LAN-only or outbound-friendly; email runs on SMTP or degrades to a clean no-op. Only billing degrades — to unlimited.
6. **Outbound-only, storage-free, NAT-friendly.** The desktop dials out — to GitHub via the App token model (no stored secrets), to the relay for steering, to the push service. Agents work behind any NAT with zero inbound ports and zero friction.
7. **Legible from phone to desktop.** One issue = one PR = one worktree = one steerable session, with `agent_runs` as the single synced source of truth. The fix stays readable on any screen.
8. **Five-client Electric lockstep.** Parity is a discipline, not an aspiration. macOS stays glass; Linux reaches web pixel-parity and leaps ahead with a real diff view. The same shapes sync everywhere or they don't ship.

---

## What we are NOT

This list is a feature, and we're proud of it. If it's here, we will not build it — and if it's half-present, it comes out:

- **No Kanban drag-drop board**
- **No saved filters / custom saved views**
- **No cycles / sprints**
- **No sub-issues / dependency graphs**
- **No time tracking, estimates, or custom fields**
- **No bulk edit**
- **No issue templates**
- **No agent marketplace or MCP server browser**
- **No presence / typing indicators**
- **No timeline / Gantt**
- **No public roadmap share**
- **No Linear import**
- **No Google Calendar** — fully excised, columns and sync paths gone

We also delete our own past when it stops earning its keep: legacy agent-auth C symbols, the `companion.*` alias, and dead sync paths all go. Lean schema, lean surface.

What we **do** keep — the few high-leverage relational wins users genuinely loved:

- **Issue-to-issue linking** — reference other issues as clickable pills inside descriptions and comments, resolved like @mentions.
- **Duplicates** — mark an issue as a duplicate of its canonical, collapsing noise into one source of truth. No relation graph, no generic link table — just the good parts.
- **My Issues** — one cross-project view of what's assigned to you, first-class on web, iOS, and Android.

---

## Who it's for

Small-to-mid engineering teams — and solo builders — who want Linear's speed and clarity without Linear's price, its per-seat billing, or its endlessly expanding surface area. People who ship code and are tired of tools that stop at *tracking* the work. People who want to self-host without losing features. People who want an issue to arrive, get assigned to an agent, and come back as a merged PR — steered from wherever they happen to be standing.

---

## The business moat

**Cheaper and simpler than Linear — billed per workspace, never per seat.** A flat workspace rate means adding teammates never taxes you for collaborating. Notifications (email + push) are free on every tier because losing work should never be a paywall. We monetize where value actually compounds: **agents, seats-as-capacity, repositories, and workspace tier.**

Self-hosted supports every feature at full parity; billing is the only thing that degrades, and it degrades to unlimited. The simplicity we enforce keeps support costs low and the product learnable, which keeps it cheap to run and cheap to sell.

The defensible core isn't a feature list — it's the one flow nobody else has committed to: **issues get fixed, not just tracked, and you can drive the fix from anywhere.**
