# Exponential — 90-Day Launch & Marketing Playbook

> Written 2026-07-07, right after production go-live (`app.exponential.at`, desktop apps shipped, iOS/Android store submissions in flight).
> Audience: Danny (solo founder). Budget: **under $400 hard cash for 90 days.** This is an execution doc, not a strategy essay — do the things in order.

---

## 0. The one-sentence thesis

A solo devtool founder in 2026 wins on **founder-led X content + ONE coordinated 48-hour launch window** (Show HN + Product Hunt + directories firing together to trigger GitHub Trending) — **not** on ads, and **not** on a big-bang launch in week 1.

**Positioning line, use it everywhere:**
> *The issue tracker that closes its own issues. Free to self-host under 10 people, cloud from $0.*

Two audiences, two wedges:
1. **Self-hosters** — `docker compose up`, free under 10 people → r/selfhosted, awesome-selfhosted, selfh.st.
2. **AI-coding devs** — the killer demo: an issue that opens *its own PR* via an embedded Claude Code session → Show HN, X, YouTube.

**The hero asset the entire plan depends on** (produce it in week 1, everything consumes it):
> A 45–60s screen recording, real product, no music-over-mockup: **feedback-widget bug report → issue appears live → "Start coding" in the desktop IDE → Claude Code session → PR opens on the issue → merge → status flips to done on the phone.** Cut a 3-minute extended version at the same time for YouTube / Product Hunt.

---

## 1. License reality — READ THIS BEFORE POSTING ANYWHERE

Exponential is under the **Exponential Small Team License 1.0 (ESTL-1.0)** — *source-available*, **not open source / not FOSS**. Learn the four sentences and never improvise:

1. The source is public. You can read it, modify it, and run it.
2. Self-hosting in production is **free while your company and its affiliates have fewer than 10 total people** (employees + contractors). 10 or more → commercial licence, dennis@straehhuber.com.
3. Evaluating, developing and testing is free at **any** size.
4. Nobody may resell Exponential as a hosted or managed service.

This changes how you market:

- Say **"free & self-hostable under 10 people"** or **"source-available"**. **Never say "open source."** HN and r/opensource will eviscerate a mislabeled license, and it torches trust.
- Write a short **"Why this license, honestly"** explainer page before the launch window — it preempts the #1 HN objection, and under a bespoke headcount cap it is *more* load-bearing than a stock license would be. Cover, in this order: (a) one solo developer, no VC, the cloud is the business; (b) the cap is a **headcount** test — not seats in the product, not revenue, so nobody has to open their books; (c) it is **permanent** — say so out loud, because the obvious question is "does it convert to open source in 4 years?" and the honest answer is no, that was considered and rejected; (d) enforcement is **honor system** — no license server, no seat check, no phone-home, no nag banner; (e) what you can still do: read it, fork it, patch it, run it in production for free under 10 people; (f) what you can't: resell it as a service. Own the trade-off instead of burying it — "this is not open source, here's why I picked that" reads far better than a euphemism, and the number is generous enough to defend.
- **Naming discipline:** the license is *Exponential Small Team License 1.0*, or *ESTL-1.0* on second reference. Never describe it by reference to another vendor's licence or brand. Don't volunteer what the licence used to be — the licence is what it is, and the history isn't part of the pitch.
- **r/selfhosted:** the F/LOSS exemption to the 10% self-promo rule *does not cover you* — the license is non-free by any definition the sub uses. Plan to satisfy **both** the ~3-month account-age gate **and** the 10% participation rule. → **Start participating today** (see W1).
- **awesome-selfhosted:** requires first release > 4 months ago. `v0.1.0` was tagged 2026-03-10, so you clear the age gate ~**July 10**. But its FOSS requirement makes acceptance unlikely — submit via **awesome-selfhosted-data** with the **non-free license marker** and expect pushback. Ask the maintainers *before* burning a PR.
- r/opensource is **OUT**. r/SideProject, r/webdev showoff threads, r/homelab are **IN**.

---

## 2. Chosen plan: **Option A — slow-burn founder-led + one coordinated launch window (~week 7)**

Rejected alternatives, for the record:
- **Big-bang in week 1-2 (Option B):** fails hard prerequisites — Reddit can't post promo for ~3 months, zero X audience means the PH/HN posts have no seed engagement (thousands of visits → single-digit signups), and it burns your one-shot Show HN card on an unrehearsed pitch.
- **Paid-amplified from day 1 (Option C):** developers are ad-blind; $500 buys ~400-700 "curious, not motivated" clicks — worse ROI than one good organic demo clip. Wastes cash before message-market fit is proven.

Option A matches how PH/HN/Trending actually reward founders in 2026 (existing audience + preparation + coordinated velocity), and no single bad day can kill it.

---

## 3. THE 3 HIGHEST-LEVERAGE ACTIONS THIS WEEK

1. **Record the hero clip** (spec in §0). This is the gate for everything.
2. **Start the Reddit clock TODAY.** From your aged account, begin genuine daily participation in **r/selfhosted + r/homelab** (answer questions, **zero promo**). The promo post lands ~week 12. The F/LOSS exemption is not available to you — you need both gates cleared, so the clock has to start now.
3. **Book uneed.best** (~$29.99 to pick a date ~10 days out; DR-75 dofollow backlink on the paid tier — cheap, worth it) **and** start daily posting on **@exponential_dan** with the hero clip pinned. Also open the **awesome-selfhosted-data** question with maintainers (age gate clears ~July 10; flag ESTL-1.0 as non-free).

---

## 4. Account strategy

| Account | Role | Effort | Cadence |
|---|---|---|---|
| **@exponential_dan** (personal) | The conversion engine. Build-in-public numbers, demo clips, engineering war stories, hot takes, replies. | **80%** | 1 original post/day + **30 min of substantive replies** to Claude Code / indie-dev / self-hosting accounts. ~10 posts/week total. |
| **@exponential_app** (brand) | Release notes, changelog threads, feature GIFs, RTs of Danny + users, support. Exists so the handle is owned/searchable/credible — **not to grow.** | Low | 3-4 posts/week |

Founder accounts outperform brand accounts on X in 2025-26 (brand reach is throttled; people follow people). At 0 followers, **replies are the follower engine** — spend the 30 min/day there.

**Routing rule:** anything with a number, an opinion, or a failure → **_dan**. Anything shippy/official → **_app**. **_dan** quote-tweets **_app** releases with the personal story.

---

## 5. Five content pillars (with example posts)

**P1 — "Issues that fix themselves" demo clips** (2×/week, the differentiator)
- "POV: a user reports a bug through the feedback widget and 11 minutes later there's a PR. Full clip, no cuts. [video]"
- "Linear tracks your issues. Exponential *closes* them. Issue → Claude Code session → PR, from inside the tracker. [clip]"
- "Filed this bug from my phone at breakfast. Merged the fix from my laptop before coffee #2. The tracker did the middle part. [split-screen clip]"

**P2 — Build-in-public numbers & decisions** (2×/week, highest-engagement BIP format)
- "Week 1 of Exponential in production: X signups, Y self-host deploys, $Z MRR, 1 outage (my fault). What worked / what didn't 🧵"
- "Pricing decision I agonized over: Pro is $5/seat and yearly-only. Here's the spreadsheet logic 🧵"
- "My entire infra bill for an issue tracker with real-time sync + 4 native clients: €XX/mo on Hetzner. Breakdown 🧵"

**P3 — Engineering deep-dives** (1×/week thread + monthly blog — this is the HN fuel)
- "One markdown string, four renderers: byte-identical GFM round-tripping across TipTap (web), cmark-gfm (iOS), commonmark-java (Android), pulldown-cmark (Rust) 🧵"
- "We built the desktop app in Rust with gpui — Zed's UI framework. What that's actually like outside Zed 🧵"
- "ElectricSQL in production: 14 synced shapes, 4 clients, and the shape-handle 409 loop that almost shipped 🧵"

**P4 — Self-hosting & ownership** (1×/week, feeds the Reddit-ready reputation)
- "docker compose up and you own your issue tracker. Postgres, Electric, S3-compatible storage, done. [terminal clip]"
- "Your issue tracker knows everything about your product's weaknesses. Maybe don't rent it? Self-host guide: [link]"
- "Under 10 people? Self-host it free — your hardware, every feature unlocked. Cloud = we do the ops. Both first-class — here's how (and why that license, honestly) 🧵"

**P5 — Memes & hot takes on AI-coding workflows** (1-2×/week — YES, memes fit; keep them about the *workflow*, never punching at users)
- Drake meme: "Copy-pasting the ticket into Claude" ❌ / "The ticket IS the Claude session" ✅
- "'AI will replace developers' — meanwhile me, a developer, teaching an AI to file its own PRs so I can review code from a beach"
- Galaxy-brain escalation: bug report → Jira ticket → Slack thread about the ticket → … vs … widget report → PR.

**Ratio:** P1/P2 carry conversion, P3 carries credibility, P5 carries reach.

---

## 6. Week-by-week 90-day calendar

- **W1 (now):** Hero clip recorded (+ 3-min cut). Reddit participation starts. uneed date booked. Analytics/UTM discipline set up (per-channel UTMs + a `/go/*` redirect scheme; PostHog or Plausible on marketing + app). Pin hero clip on both X accounts. Polish the two conversion paths: `exponential.at` → cloud signup, and README quickstart → `docker compose` in < 10 min (**test on a clean VM**).
- **W2:** uneed.best launch day (reply to every comment). Blog #1 = the P3 markdown-parity deep-dive, cross-post to dev.to. Submit low-effort directories: AlternativeTo (position as Linear + Marker.io alternative), OpenAlternative/opensourcealternative.to (disclose ESTL-1.0 up front), selfh.st content tip, console.dev free listing. Start YouTube: hero Short + 3-min version.
- **W3:** First BIP numbers thread (uneed results, signups — honesty > vanity). awesome-selfhosted-data PR (age gate cleared; ESTL-1.0 flagged non-free; if rejected, that's itself a P2 post). 2 more Shorts.
- **W4:** Blog #2 (gpui/Rust desktop — strongest HN-bait). Newsletter outreach list (see `jackbridger/developer-newsletters`): pitch console.dev, Self-Hosted podcast/newsletter, smaller AI-coding newsletters (free editorial pitches only; skip TLDR — paid $2k+).
- **W5:** Launch-window prep sprint: Show HN draft, first-comment technical writeup (architecture, ElectricSQL, why ESTL-1.0 and why the cap is permanent), PH assets (gallery, 3-min video — no hunter needed in 2026 meta), FAQ. **Load-test the signup path**; verify `/api/health`, onboarding, and the feedback widget survive a spike.
- **W6:** Dry run — post blog #3 (ElectricSQL) as a *regular* HN link (not Show HN) to calibrate. Line up 10-15 friendly devs who *genuinely use it* for launch-day questions (authentic only — HN bans coordinated voting AND AI-written comments; never share direct links to the HN post).
- **W7 — THE 48-HOUR LAUNCH WINDOW (Tue-Wed, 9-11am ET):**
  - **Day 1:** Show HN + simultaneous X thread on _dan (hero clip + story) + _app release thread + posts in relevant Discords (Tuist, ElectricSQL, TanStack, gpui/Zed — you're a real member of all four) + dev.to announcement. **Danny lives in HN comments for 8 hours.**
  - **Day 2:** Product Hunt (12:01am PT) + "we launched yesterday, here's what HN taught us" thread.
  - **Goal:** concentrated star velocity → GitHub Trending (~1.4 stars per HN upvote for repo-linked tools; coordinated 48h windows are what trigger Trending).
- **W8:** Harvest — reply to everything, ship 2-3 visible fixes from feedback ("you asked, it's live"), publish "Launch retro: numbers, what worked." Re-send the retro to newsletters that ignored you in W4 (traction reopens doors).
- **W9:** Resume cadence (most launches flat-line here — 1 substantial piece/week holds baseline growth). Blog #4: steer-relay / remote-steering architecture. First **widget-wedge** campaign: "Add a feedback widget in 5 min, reports become tracked issues" clip + docs page targeting marker.io searchers.
- **W10:** Mobile-store angle when approvals land ("the only issue tracker with native apps on every platform + a Rust desktop IDE"). Shorts push (3× native-app demos).
- **W11:** Guest content: 1 podcast pitch (Self-Hosted, devtools-founder pods) + 1 guest post. Monthly changelog on _app, QT'd with commentary on _dan.
- **W12:** **r/selfhosted promo post** (account now ~90 days old with real history): lead with self-hosting, disclose ESTL-1.0 and the 10-person cap in-body, answer every comment for 24h. Cross-post r/SideProject + r/webdev.
- **W13:** 90-day retro thread (the full numbers). Decide the ads experiment on data. Plan days 91-180: **SEO comparison pages** (Exponential vs Linear / vs Jira / vs Marker.io — the compounding channel).

---

## 7. Ads verdict ($)

- **Days 1-60: $0.** X CPC ~$0.74-1.50 (below Meta), but devtools buyers are ad-blind. A founder reply-guy hour beats $50/day.
- **Days 60-90 (optional):** only if a clip already went top-decile organically — **$200-300 total, $10/day**, boosting *that* clip, targeting follower-lookalikes of @linear, @cursor_ai, @AnthropicAI, @zeddotdev. **Kill if blended cost/signup > $10.** Never run "follower" campaigns.
- **Redirect the unspent budget:** uneed paid slot ($30, W1) + hold ~$300 for ONE small newsletter/podcast sponsorship in month 4, once the funnel demonstrably converts.

**Total 90-day cash: ~$30 uneed + $0-300 optional ads + ~$0 everything else = under $350.**

---

## 8. Metrics — weekly dashboard, keep it to 8

1. Cloud signups/week **by UTM source**
2. Activation rate (workspace with ≥1 project + ≥5 issues within 7 days)
3. Self-host deploys proxy (docker pulls / unique `/api/health` origins if measurable, else README-quickstart page hits)
4. GitHub stars + **weekly velocity**
5. @exponential_dan followers + impressions
6. **Coding sessions started/week** (the aha-moment metric — `coding_sessions` table)
7. Widget configs created (wedge adoption)
8. MRR + paid seats

**North star for 90 days: weekly activated workspaces.** Vanity to ignore: PH upvotes, uneed rank, brand-account followers.

---

## 9. Launch-window prerequisites checklist (gates W7)

- [ ] Hero clip done (W1)
- [ ] Onboarding survives a stranger with no help (test W5)
- [ ] Show HN first-comment writeup drafted (W5)
- [ ] "Why this license" explainer page live (permanent 10-person cap, honor system — preempts the #1 HN objection)
- [ ] Signup path load-tested (W5)
- [ ] 10-15 authentic supporters briefed (W6)
- [ ] Reddit promo does **NOT** happen in this window — that's the W12 second spike

---

## 10. Directory / channel cheat-sheet

| Channel | When | Notes |
|---|---|---|
| uneed.best | W1 book / W2 launch | Free queue or ~$29.99 pick-your-date + DR-75 dofollow backlink. Worth the $30. |
| AlternativeTo | W2 | Position as Linear + Marker.io alternative |
| OpenAlternative / opensourcealternative.to | W2 | Disclose ESTL-1.0; accept probable rejection |
| console.dev | W2 | Free listing submission |
| selfh.st | W2 | Content tip |
| dev.to | W2, W4, W7 | Cross-post every blog |
| awesome-selfhosted-data | W3 (age clears ~Jul 10) | Non-free license marker; ask maintainers first |
| YouTube (Shorts + long) | W2 onward | Hero clip is the seed |
| Newsletters | W4 pitch | `jackbridger/developer-newsletters` catalog; free editorial only |
| **Show HN** | **W7 Day 1** | Tue-Thu 9am-12pm ET, plain technical title, founder in comments 6-8h |
| **Product Hunt** | **W7 Day 2** | Saturated (500+/day) — only works inside the multi-channel window |
| r/selfhosted | W12 | Account-age + participation gates; lead self-hosting, disclose ESTL-1.0 |
| X ads | W9-13 optional | Only on a proven clip, ≤$300, kill at >$10/signup |
