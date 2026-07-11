# Exponential — Vision

**The Linear-simple issue tracker that actually fixes the issue.**

---

## The one-sentence pitch

Exponential is the ultimate Linear alternative — simpler and cheaper — that is also a full coding IDE with embedded AI agents, so issues don't just get tracked, they get **fixed**: clone → AI → PR, driven from anywhere, even your phone.

---

## What Exponential is

Exponential is an issue tracker with the simplicity of Linear — and simplicity *is* the moat, not a limitation we apologize for. You get a fast, real-time tracker: issues, projects, labels, priorities, comments, My Issues, an inbox. Nothing you have to learn. Nothing you have to configure. It syncs live across web, iOS, Android, and the desktop app (macOS, Windows, and Linux).

But Exponential has a superpower Linear will never have: **the fix flow is built in.** Every issue can become a coding task. Click **Start coding** on an issue and Claude opens right there in the desktop's embedded terminal — seeded with a plan-first prompt (issue title, description, relevant comments) and told to propose a concise plan, wait for your go-ahead, then implement. Permissions are bypassed, so you never babysit accept prompts; it's fully interactive from the first keystroke. It works in a dedicated git worktree, and when it's done it commits, pushes the `exp/<IDENTIFIER>` branch, and opens its own pull request. One issue = one PR = one worktree. The desktop app is a real IDE around your project's repo: every project is backed by exactly one repository, which the desktop **auto-clones** on open — so terminals open *in the repo*, run configs actually run, and a git top bar (pull/push with auto-rebase), a full source-control panel (stage, commit, history, diffs), and a read-only file tree complete the picture. When a pull hits conflicts, one button — **Fix conflicts with Claude** — opens an interactive session that resolves them in front of you. Issue worktrees stay Claude's domain: you review them as a live **Changes** tab inside the issue, never by juggling checkouts. The web and mobile apps are the coordination and *remote-control* surface: you can create and triage issues, review the same Changes tab (PR diff, or the pushed branch before a PR exists), and — the part nobody else does — **watch a live coding terminal running on your desktop and type into it, from your phone.**

Because the fix flow lives at the center, Exponential is **coding-first**. Every project is created with its backing GitHub repository — project and repo are one-to-one and mandatory, so there is no such thing as an issue with nothing to code against. GitHub isn't an integration you might configure; it's the ground the product stands on.

And when work reaches an end-user — a bug filed through the embedded feedback widget — Exponential closes the loop with a **built-in one-way helpdesk**: the reporter becomes a subscriber and gets an automatic resolution email the moment their issue is done. Support, without buying a support tool. This is **the loop** the whole product exists to close, and the marketing line that carries it: **"Make your app exponential."** — a user reports a bug through your app's widget → it lands as an issue → Claude codes the fix in your IDE → the PR ships → the reporter gets told it's fixed. No other tool owns that circle end to end.

Everything works fully **self-hosted**. Every feature. The only thing that degrades on self-hosted is billing — and it degrades to unlimited.

---

## Platform roles

The split is locked and load-bearing. The desktop is where code runs. The web and mobile clients coordinate and remote-control — they never host a shell or bundle a CLI.

| Surface | Role | Has |
| --- | --- | --- |
| **Desktop** — one cross-platform app (Rust: gpui.rs + gpui-component), pixel-parity with web, on macOS, Windows, and Linux | The **IDE**. Where coding sessions and builds actually run. | Auto-cloned project repo (the trunk), git top bar (pull/push, auto-rebase, conflict mode → **Fix conflicts with Claude**), full source-control panel (stage/commit/history/diffs), read-only file tree + viewer, embedded terminal (alacritty_terminal, over a PTY the app owns directly) opening *in the repo*, JetBrains-style run configs + play button, concurrent coding sessions, multi-window, per-issue live **Changes** tab, and it is the steer-relay **publisher** that tees the live session to your phone |
| **Web** + **Mobile** — iOS (native glass) · Android (Compose) | The **coordination + remote-control** surface. | Create/triage/assign, comments, My Issues, Inbox, the same per-issue **Changes** tab (PR diff → pushed-branch diff → live-session watch/steer), **watch + steer a live desktop coding session** — no local git, no local terminal, no bundled CLI |

The desktop runs Claude interactively inside an embedded terminal (alacritty_terminal, over a PTY the app owns directly) — one terminal, one worktree, one child process per session. There is no headless loop and no shared runtime: each window is just its own terminal driving its own `claude` in its own worktree. Because the desktop owns the PTY master, it tees the live session out to the relay for phone steering with no extra plumbing. Web and mobile stay pure coordination surfaces on purpose.

---

## The killer flow

An issue arrives while you're out. Your desktop — signed in and online — is running at home.

1. The issue **emails your phone** (email is a first-class delivery channel, free on every tier). You open it in the mobile app.
2. From your phone, you tap **Start on my desktop**. A start-session command travels over the relay to your online desktop, which resolves the repo from the workspace repository registry, spins up a git worktree at home, and opens Claude in an embedded terminal — seeded with a plan-first prompt.
3. You **watch the live terminal** stream from your desktop, on your phone. Claude proposes its plan and waits; you **type into it** to approve, redirect, or answer its questions. Full bidirectional steering, not read-only status. The relay carries the bytes; a live coding-session record tells every client the work is happening.
4. Claude commits, pushes, and opens a PR itself. You **review the syntax-highlighted side-by-side diff** on the same phone.
5. It merges. You never sat down at a computer.

That's the moat in one story: an issue went from *reported while you were away* to *merged PR* — steered from your pocket. Linear can track the issue. Exponential fixes it.

---

## Core principles

1. **Simpler than Linear is the moat.** The features we *refuse* to build are the product's competitive edge. Every "no" keeps the tool learnable in minutes and defensible for years.
2. **One great flow, not a hundred features.** The clone → AI → PR loop is the one path we polish relentlessly and never scatter around. We invest in that single path instead of a hundred shallow features.
3. **Coding-first / GitHub-mandatory.** Project ↔ repository is one-to-one and required at creation — an issue without something to code against cannot exist. Two git worlds keep it legible: the IDE chrome is the trunk, always; issue worktrees belong to Claude and surface only inside their issue.
4. **Nothing gets lost — and we never monetize on that.** In-app + push + email fan-out is table-stakes and **free on every tier**, and so is watching + steering your own desktop from your phone. We monetize seats and the feedback loop — never anxiety, never notifications, never the demo.
5. **Self-hosted-first parity.** Every feature works self-hosted at full fidelity. The relay works LAN-only or outbound-friendly; email runs on SMTP or degrades to a clean no-op. Only billing degrades — to unlimited.
6. **Outbound-only, storage-free, NAT-friendly.** The desktop dials out — to GitHub via the App token model (no stored secrets; a JIT installation token is fetched per session), to the relay for steering, to the push service. It works behind any NAT with zero inbound ports and zero friction.
7. **Legible from phone to desktop.** One issue = one PR = one worktree = one steerable session — coordination clients read the issue's PR plus a live coding session, so the fix stays readable on any screen.
8. **Four-surface Electric lockstep.** Parity is a discipline, not an aspiration. Web, iOS, Android, and the gpui desktop app all sync the same fifteen shapes — the same shapes sync everywhere or they don't ship. The desktop is the one surface that runs coding sessions and publishes to the steer relay; the others coordinate and remote-control. The desktop reaches pixel-parity with web (shadcn look via gpui-component) and leaps ahead with a real embedded terminal and a syntax-highlighted side-by-side diff view.

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
- **No headless / background agents** — coding is interactive-in-the-terminal only; you watch and steer, nothing runs plan-only in the dark (even conflict fixing is a visible Claude session you can type into)
- **No editable code editor** — Claude and the terminal do the editing; the IDE's file tree, viewer, and diffs stay read-only on purpose
- **No Google Calendar** — fully excised, columns and sync paths gone

We also delete our own past when it stops earning its keep: the old Rust agent-core and its FFI bridges, the synthetic desktop-agent user and device registration, the assignment-trigger machinery, and dead sync paths all go. Lean schema, lean surface.

What we **do** keep — the few high-leverage relational wins users genuinely loved:

- **Issue-to-issue linking** — reference other issues as clickable pills inside descriptions and comments, resolved like @mentions.
- **Duplicates** — mark an issue as a duplicate of its canonical, collapsing noise into one source of truth. No relation graph, no generic link table — just the good parts.
- **My Issues** — one cross-project view of what's assigned to you, first-class on web, iOS, and Android.

---

## Who it's for

Small-to-mid engineering teams — and solo builders — who want Linear's speed and clarity at a fraction of Linear's price, without its endlessly expanding surface area. People who ship code and are tired of tools that stop at *tracking* the work. People who want to self-host without losing features. People who want an issue to arrive, hit **Start coding**, and come back as a merged PR — steered from wherever they happen to be standing.

---

## The business moat

**Free for individuals forever. Teams pay per seat — at roughly half of Linear's price.** A solo builder gets the entire product (unlimited projects, repos, coding sessions; storage-capped) for $0, because solo users cost us ~nothing and become the teams that pay. When you invite your first teammate you buy seats: Pro at $5/seat/month (billed yearly), Business at $10/seat/month — no artificial caps on projects, repositories, or how many terminals you open. We refuse to meter the work itself.

Notifications (in-app, push, email) and remote watch/steer are free on every tier — losing work should never be a paywall, and we don't paywall our own best demo. What teams pay for beyond seats is **the loop**: the embeddable feedback widget and its helpdesk resolution emails — the piece an individual doesn't need but a team with a live app happily pays for.

Self-hosted supports every feature at full parity; billing is the only thing that degrades, and it degrades to unlimited. For larger self-host organizations there's an Enterprise support tier — contact sales, honor system. The simplicity we enforce keeps support costs low and the product learnable, which keeps it cheap to run and cheap to sell.

The defensible core isn't a feature list — it's the one flow nobody else has committed to: **issues get fixed, not just tracked, and you can drive the fix from anywhere.**
