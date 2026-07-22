# SES production access — denial analysis & resubmission plan (EXP-114)

**Status:** the AWS account is in the SES **sandbox** in **eu-north-1**. The first
production-access request (filed ~2026-07-15) was denied on 2026-07-22 with the generic
letter citing the AUP, the Service Terms, and the SES best-practices guide. AWS never
states specific reasons — the generic letter is the signature of a request that failed
the first-line review, not of a permanent judgment.

This doc is the plan of record for attempt 2: why we were probably denied (§1), what
changed since the original request (§2), the app/ops changes to make before resubmitting
(§3), the ready-to-send appeal letter (§4), and process guidance (§5). The original
request text and the denial letter are preserved in the EXP-114 comment thread.

---

## 1. Why we were (probably) denied — ranked

### 1.1 The region error (most fixable, possibly decisive)

The original request text told AWS our sending region was **eu-central-1**. Our verified
identity, credentials, and all sending live in **eu-north-1** (the sandbox rejection
itself says `EU-NORTH-1`). SES production access is granted **per region**, and reviewers
cross-check the request against the account: a request describing eu-central-1 pointed
them at a region with **no verified domain, no DKIM history, and no sending record** — an
empty account asking for production access. There is a documented re:Post case where
exactly this mismatch drove a denial. The resubmission must be filed from the eu-north-1
console and explicitly correct the error.

### 1.2 The stranger-address streams vs. the opt-in acknowledgement

The request form's acknowledgement commits us to *"only send email to individuals who've
explicitly requested it."* Two of our streams, read baldly, contradict that:

- **Widget reporter mail** (answer to "could it be the feedback widget?": **yes, a probable
  contributor**, on two axes). (a) *Unverified addresses*: a stranger types an email into a
  form on a **third-party site** and we mail it — submit confirmation, every support reply,
  a resolution notice — with no verification step. Typos hard-bounce; anyone can type
  someone else's address. This is the classic form-to-email abuse vector reviewers are
  trained to flag, and AWS's own best-practices page names **double opt-in** as the expected
  mitigation. (b) *Framing*: "an embeddable widget third parties paste into their sites"
  pattern-matches the **agency/ESP "sending on behalf of others"** heuristic — a documented
  near-automatic denial. Both axes are fixable: §3 adds engagement gating (de-facto
  confirmed opt-in), and §4 reframes the stream as *our customers' support channel where
  the recipient submits a request and receives their own ticket confirmation* — the same
  class as an order confirmation, the strongest approved category.
- **Team invite mail**: the recipient never opted in. It's the industry-standard SaaS invite
  shape (owner types one address, single-use 7-day link, no free-text message field, no
  reminders), but
  the AUP has no transactional carve-out on its face — the framing and the abuse controls
  have to be stated explicitly.

### 1.3 The request predated the machinery it needed to describe

The Resend→SES migration landed 2026-07-15 14:29 UTC (`0a9df215`) — **hours before the
request was filed**, so the reviewer saw an account with essentially zero SES history. The
request also said SNS bounce notifications were *"being implemented"*: AWS explicitly
discounts future plans and grades **already-implemented** processes ("we may consider …
the policies and processes you have in place" — AUP enforcement clause). The bounce
pipeline has since shipped (§2), and the resubmission must describe everything in the
present tense.

### 1.4 Account-trust heuristics we can't see

Practitioner consensus: young accounts, thin billing history, and low spend weigh
invisibly against approval regardless of request quality. Mitigations: keep sandbox
sending to verified team addresses so the account accrues clean history, resubmit once
(strong), and escalate through humans rather than spraying attempts (§5).

### 1.5 What is NOT the problem

The digest posture (push-first, daily default, verified members only, RFC 8058 one-click
unsubscribe, failure backoff), the volume (<100/day single-recipient sends), the zero
marketing/lists posture, and site legitimacy (exponential.at + app.exponential.at, privacy
policy in the marketing footer) are all *strengths* — they lead the appeal, not the fixes.

---

## 2. What changed since the original request

All of this postdates (or was omitted from) the denied request and belongs in the
"what changed" section of the appeal — including the two **new email categories** that
must be disclosed (an undisclosed stream that later spikes complaints is how accounts get
re-sandboxed).

| Change | When / evidence |
|---|---|
| Resend → SES migration itself (SESv2 `SendEmail`, single-recipient) | 2026-07-15, `0a9df215` |
| Contact-form Reply-To fixed (SES rejects Reply-To as a custom header) | 2026-07-15, `04e4832e` (EXP-110) |
| Digest default flipped **hourly → daily** (at most one email/user/day; hourly is an explicit opt-in) | 2026-07-15, `3eb519d4` (EXP-115) |
| SES **bounce/complaint pipeline shipped**: SNS topic → HTTPS webhook (`/api/webhooks/ses`), per-address `email_bounces`, per-message `email_deliveries` stamping by SES MessageId | 2026-07-21, `98ea83e1` (EXP-227) |
| Admin console **suppression worklist** + one-click push to the SES account-level suppression list (`PutSuppressedDestination`) | 2026-07-21, `98ea83e1` (EXP-227) |
| Failed digest sends back off to **≤1 retry/day** (no more retry storms against a failing transport) | 2026-07-21, `98ea83e1` (EXP-227) |
| Digest recipient gate: address present AND **email_verified** AND **still a team member** (ex-members' rows claimed without sending) | 2026-07-12 `a106eed1` (REV-65) + 2026-07-21 `d7105f09` (REV2-14) |
| Unsubscribe endpoint hardened: GET renders confirm page, only POST mutates (scanner-safe), RFC 8058 one-click POST path intact | 2026-07-11, `6a496913` (REV-20) |
| Public contact identity moved onto the sending domain (danny@exponential.at everywhere) | 2026-07-21, `5372c07e` (EXP-226) |
| **NEW stream — helpdesk reporter mail**: support confirmation + reply emails to widget reporters (magic conversation link) | 2026-07-15, `ed9b4c04` (EXP-123) + `479f8388` (EXP-130) |
| **NEW stream — team invite email**: owner-typed address, single-use 7-day join link | 2026-07-19, `b66c0122` (EXP-188) |
| Auth posture confirmed: cloud runs **Google + Apple sign-in only** — the password-reset and verification flows described in the original request never send in production | posture since 2026-06-09 |

---

## 3. App-change plan before resubmitting

### P0 — ship before the appeal (every letter claim must be present-tense)

1. **Automatic send-time suppression check.** No send path consults `email_bounces` today —
   suppression is only SES-account-level plus the manual admin action. Add a guard in the
   shared sender (`apps/web/src/lib/email.ts` `sendEmail`) that skips any address with a
   `Permanent` bounce or a complaint on record in `email_bounces` (return
   `{delivered:false}` with a `suppressed` reason so ledgering callers record it). This
   unlocks the single sentence reviewers look for: *"we never attempt a second send to an
   address that has hard-bounced or complained."* Covers every stream uniformly, including
   the SMTP self-host transport.
2. **Complaint auto-opt-out.** A complaint currently only lands in the worklist; the user
   keeps receiving digests until an admin acts. In the SES webhook
   (`apps/web/src/routes/api/webhooks/ses.ts` `recordEmailBounceEvents`), match complaint
   addresses to `users.email` and set `user_notification_prefs.email_enabled = false`
   (idempotent; the user can re-enable in account settings).
3. **Reporter engagement gating (de-facto confirmed opt-in).** No schema change needed:
   `support_threads.last_reporter_seen_at` already records when the reporter last loaded
   the magic-link page. Gate `sendSupportReplyEmail`
   (`apps/web/src/lib/trpc/helpdesk.ts`) on it being non-null: replies only email an
   address whose owner has opened their conversation link at least once. Result: an unconfirmed (typo'd,
   maliciously entered) address receives **at most one email, ever** (the submit
   confirmation carrying the link). The feedback-mode resolution notice
   (`apps/web/src/lib/integrations/notifications.ts`) already IS the only email ever sent
   to that address — keep it exactly-once, now also suppression-checked via item 1, and
   present it that way.
4. **Per-recipient-address caps.** (a) Invites: cap invite emails per recipient address
   platform-wide (e.g. 3/week) in `teamInvites.create` — closes the "invite-bombing"
   reading of the AUP's facilitation clause. (b) Widget: add a per-recipient-address
   bucket to the existing per-key/per-IP limits so one address can never be mail-bombed
   via repeated submits.
5. **Ledger invite sends.** Invites write no `email_deliveries` row today, so their bounces
   can't be traced per-message. Add `kind: 'team_invite'` ledger rows in
   `apps/web/src/lib/trpc/team-invites.ts`.
6. **Monitored reply path.** All mail is from `noreply@exponential.at` — literally named as
   an anti-pattern on the best-practices page the denial cited. Cheapest fix: set
   `EMAIL_FROM` to `Exponential <notifications@exponential.at>` (or keep noreply@ and add a
   default monitored `replyTo`, e.g. danny@exponential.at, in `sendEmail`).

### P1 — AWS console / DNS (Danny, no code)

1. Confirm the **domain identity** `exponential.at` (not just the address) is verified in
   **eu-north-1**, with **Easy DKIM** CNAMEs published and status Verified.
2. Add a **custom MAIL FROM** subdomain (e.g. `mail.exponential.at`) so SPF aligns.
3. Publish a **DMARC** record on exponential.at (at least `p=none` with `rua=` reporting) —
   DNS is on Cloudflare, minutes of work. Since 2024 AWS effectively expects
   SPF+DKIM+DMARC before granting production access, and DNS is one of the few claims a
   reviewer can independently verify.
4. Verify the **account-level suppression list** is enabled for BOTH bounces and complaints
   (SES console → Suppression list settings).
5. Confirm the prod **SNS subscription** to `/api/webhooks/ses?secret=…` is live and
   confirmed (SES_WEBHOOK_SECRET set on the Coolify web app; send a test bounce to
   `bounce@simulator.amazonses.com` from the sandbox and watch the admin Email tab).
6. Enable **event publishing** (the console's open "Monitor email sending" task): create a
   default **configuration set** with an event destination to CloudWatch (sends,
   deliveries, bounces, complaints) and set it as the account default. It complements the
   identity-level SNS webhook, feeds the alarms below, and is a visible "monitoring is
   configured" signal for the reviewer.
7. Create **CloudWatch alarms** on the account `Reputation.BounceRate` / 
   `Reputation.ComplaintRate` metrics (alert at e.g. 2% / 0.05%, well under AWS's review
   thresholds), delivering to an address you read.

**Explicitly skip** two other console suggestions: the **Virtual Deliverability Manager**
(paid add-on aimed at high-volume senders — at <100/day it adds nothing over the free
reputation metrics, and it has no bearing on the production-access review) and
**dedicated IPs** (actively harmful at our volume: a dedicated IP can never warm up on
<100 emails/day, so the shared SES pool delivers better, and it costs ~$25/mo per IP).

### P2 — optional hardening (not blocking the appeal)

- Widget submit-time **MX validation** of the reporter address (AWS explicitly recommends
  it for web forms).
- Digest **dormancy stop** (suspend after N consecutive digests with zero sign-ins; resume
  on next login — engagement-based list hygiene).
- Move sending to a **dedicated subdomain identity** so the reporter stream can't drag the
  apex domain's reputation.
- **SNS signature verification** on the webhook (today: shared-secret query param only).
- Unsubscribe **token rotation** (static UUID today; leak = permanent silence for that user).

---

## 4. The appeal letter

**How to send:** reply to the **existing denied support case** (Support Center → the case →
reply) — the documented reversal path is a rewritten use case in the same case, which
triggers escalation to secondary review. Only if the case is locked: file fresh from the
**eu-north-1** console (Account dashboard → Request production access) or via
`aws sesv2 put-account-details --production-access-enabled --mail-type TRANSACTIONAL
--website-url https://exponential.at …`. Send **only after P0 + P1 are done** — every claim
below must be true at send time. Attach/inline: sample emails for each stream **with full
headers** (digest sample must show `List-Unsubscribe` + `List-Unsubscribe-Post`), the live
unsubscribe URL, screenshots of the suppression-list setting, the SNS topic + confirmed
HTTPS subscription, and the admin suppression worklist.

> **Subject: Request for re-review — SES production access, eu-north-1 (transactional)**
>
> Hello,
>
> Thank you for reviewing our earlier request. We'd like to ask for a re-review, because
> we found a material error in our own request, and because several controls that were
> in progress at the time have since shipped and are live in production.
>
> **Correction:** our previous request stated the sending region as eu-central-1. That was
> our mistake — all our verified identities and all sending are in **eu-north-1** (domain
> identity exponential.at, Easy DKIM verified, custom MAIL FROM mail.exponential.at with
> aligned SPF, DMARC published). We ask that this request be evaluated against eu-north-1.
>
> **Who we are:** Exponential (https://exponential.at) is an issue tracker operated by an
> Austrian company; the product runs at https://app.exponential.at (privacy policy linked
> from both). Amazon SES is used exclusively for **transactional, single-recipient** email
> triggered by individual user actions, sent one at a time via the SESv2 SendEmail API.
> We send **no marketing, no newsletters, never use the bulk API, and maintain no mailing
> lists** — no purchased, rented, imported, or scraped addresses, ever. Current volume is
> **well under 100 emails/day**; growth follows our user base gradually, with no bursts.
>
> **What changed since our previous request:**
> - The notification digest default changed from hourly to **daily** — at most ONE bundled
>   email per user per day, and only for notifications still unread an hour after the
>   push notification already delivered them.
> - Our bounce/complaint pipeline is now fully live (described below) — it was in
>   progress at the time of the first request.
> - We added an **automatic send-time suppression check**: our application never attempts
>   another send to any address with a recorded hard bounce or complaint, on top of the
>   SES account-level suppression list.
> - A spam complaint now **automatically disables** all notification email for that
>   account, without waiting for an operator.
> - Support correspondence to externally provided addresses is now **engagement-gated**
>   (described below): an address that never confirms receives at most one email, ever.
>
> **Every email we send, and how each recipient's address was obtained:**
>
> 1. **Notification digest** (largest stream). Recipients are our own authenticated users —
>    accounts exist only via Google or Apple sign-in, so every address is
>    provider-verified; we additionally require the address to be verified and the user
>    to currently be a member of the team the notification came from. Push notifications
>    fire immediately; email is only a daily catch-up of items still unread — nothing is
>    emailed per-event. Every digest carries RFC 8058 one-click unsubscribe headers
>    (List-Unsubscribe + List-Unsubscribe-Post) plus a visible footer link; unsubscribing
>    takes effect immediately and can also be managed per-type in account settings.
> 2. **Team invitations.** A team owner types one colleague's address; we deliver a single
>    join link (single-use token, 7-day expiry). One email per invite — no reminders, no
>    sequences, no free-text message from the sender (the email contains only the team
>    name, the inviter's name, and the join link), no address-book import, and no bulk
>    invite surface exists in the product. Invite emails per recipient address are rate-capped
>    platform-wide.
> 3. **Customer support correspondence.** Our customers offer a support/feedback form to
>    their end users. A person submits a request and enters **their own** address to
>    receive the answer: they get one confirmation email containing their private
>    conversation link. Further replies from the support team are emailed **only after
>    the recipient has opened that conversation link** — an address that never engages
>    receives at most that one confirmation, ever. Feedback reports receive at most a
>    single one-time "your report was resolved" notice, sent exactly once and
>    suppression-checked. Submissions are rate-limited per site key, per IP, and per
>    recipient address, with bot protection.
> 4. **Contact form** on our marketing site, delivered only to our own company address.
>
> Our production sign-in is Google/Apple only, so we send no password-reset or
> verification email. All mail comes from our verified domain with a monitored reply
> address.
>
> **Bounce and complaint handling (live today):** the SES account-level suppression list
> is enabled for bounces and complaints. Additionally, an SNS topic delivers every bounce
> and complaint notification to our application's HTTPS endpoint, where each event is
> recorded per-address and matched to the originating message via the SES Message ID in
> our per-message delivery ledger. Hard bounces and complaints are automatically excluded
> from all future sends by an application-side check before every send; complaints also
> automatically disable that account's notification email. Failed sends back off to at
> most one retry per day. We monitor account bounce/complaint reputation metrics with
> CloudWatch alarms set well below AWS's review thresholds.
>
> We've attached sample emails for each stream including full headers, the live
> unsubscribe endpoint, and screenshots of the suppression configuration and our
> delivery ledger. If any specific aspect of our sending program falls short of your
> criteria, we would genuinely welcome the chance to address it concretely — given that
> every message we send is a single-recipient response to an action its recipient took,
> we'd appreciate understanding how our sending differs from other transactional senders
> in production.
>
> Thank you for your time.

---

## 5. Process notes

- **One strong attempt beats many weak ones.** Denial on attempt 1 is common; documented
  successes come on a substantially more detailed attempt 2, or attempt 3 after human
  escalation. Repeated unchanged resubmissions risk a terminal "final rejection", and
  retrying from fresh AWS accounts is flagged and can trigger account closure.
- **Escalation ladder** if attempt 2 fails: (1) reply in-case asking what specifically to
  address; (2) buy Developer Support (~$29/mo, cancellable) and open a technical case with
  chat/phone; (3) any AWS account-manager / Activate contact — internal advocates have
  reversed these. re:Post can't approve anything but occasionally attracts an AWS employee
  to nudge a case.
- **Meanwhile:** keep sending in the sandbox to verified team addresses so the account
  accrues age and clean metrics (both weighed invisibly). Verify danny@/dennis@ addresses
  as identities so the contact form delivers while sandboxed (the interim option from the
  issue description still stands).
- **Timing:** initial requests are answered within ~24h; re-reviews take one to a few
  days; if a fresh submission is refused mechanically, wait ~two weeks.
- Production access is **per-region** — if sending ever moves off eu-north-1, a new request
  is needed there.

**Key sources:** AWS AUP (aws.amazon.com/aup), Service Terms §15 (SES), SES best practices
+ enforcement FAQs (docs.aws.amazon.com/ses/latest/dg/best-practices.html,
faqs-enforcement.html, request-production-access.html), AWS Messaging blog on
sandbox→production for large senders, and community postmortems (alex-dawkins.com 2025
case-reply reversal; usewaypoint.com, dev.to/aws-builders, docs.emaildelivery.com on
denial heuristics).
