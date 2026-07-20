// closedloop/fixtures.ts — the ONE fixture world of the ClosedLoop film:
// a visitor on acme.shop hits a dead Pay-now button, reports it through the
// embedded feedback widget, the issue lands as EXP-151 on the Acme Shop board,
// Claude fixes it from the Start-coding dialog, the PR merges, and the film
// closes on the Shipped card + every-platform lineup (EXP-200). Everything
// here is deterministic copy — no divergent content.

import type { BoardRow, DiffRow, SessionEvent } from "../ships/fixtures"

// ── Identity (the team whose product is acme.shop) ────────────────────────────
export const CL = {
  siteUrl: "acme.shop/checkout",
  brand: "ACME SHOP",
  reporter: "Jamie Lee",
  reporterEmail: "jamie@lee.dev",
  project: "Acme Shop",
  projectColor: "#6366f1",
  repo: "acme/shop",
  branch: "exp/EXP-151",
  pr: 218,
  runConfig: "Storefront",
  user: "Riley Chen",
  initials: "RC",
  sessionTab: "Fix the dead checkout button",
} as const

export const CL_LABELS = {
  bug: { name: "bug", dot: "#ef4444" },
  web: { name: "web", dot: "#6366f1" },
  widget: { name: "widget", dot: "#22c55e" },
  infra: { name: "infra", dot: "#3b82f6" },
} as const

// ── The acme.shop checkout page (dark third-party site) ──────────────────────
export const SITE = {
  nav: ["New in", "Men", "Women", "Sale"],
  cart: "Cart (2)",
  contactLabel: "Contact",
  email: "jamie@lee.dev",
  shippingLabel: "Shipping address",
  name: "Jamie Lee",
  address: "418 Bergamot Lane",
  cityRow: "Portland, OR 97204",
  paymentLabel: "Payment",
  card: "4242 4242 4242 4242",
  expiry: "08 / 29",
  cvc: "···",
  summaryLabel: "Order summary",
  items: [
    { name: "Fjord Parka", variant: "Slate · M", price: "$148.00", tint: "#94a3b8" },
    { name: "Trail Socks", variant: "2 pairs", price: "$24.00", tint: "#d6b88a" },
  ],
  subtotal: ["Subtotal", "$172.00"],
  shipping: ["Shipping", "$8.00"],
  total: ["Total", "$180.00"],
  payLabel: "Pay now",
  secure: "Secure checkout · 256-bit TLS",
} as const

// ── The widget report ─────────────────────────────────────────────────────────
export const REPORT = {
  panelTitle: "Send feedback",
  titleLabel: "Title",
  title: "Checkout button does nothing",
  detailsLabel: "Details",
  details: "Clicked “Pay now” on Safari — no response.",
  send: "Send feedback",
  sending: "Sending…",
  successTitle: "Thanks — sent!",
  successSub: "Tracked as EXP-151",
  poweredBy: "Powered by Exponential",
} as const

// ── Board at story start (EXP-151 pops in via insertAt) ──────────────────────
export const NEW_ISSUE_ID = "EXP-151"

export const CL_BOARD: BoardRow[] = [
  { id: "EXP-148", title: "Migrate product search to Typesense", status: "in_progress", priority: "high", label: CL_LABELS.infra, assignee: CL.initials, due: "Jul 18" },
  { id: NEW_ISSUE_ID, title: REPORT.title, status: "todo", priority: "none", label: CL_LABELS.widget },
  { id: "EXP-149", title: "Add Apple Pay to checkout", status: "todo", priority: "medium", label: CL_LABELS.web, assignee: CL.initials },
  { id: "EXP-150", title: "Order confirmation email renders twice", status: "todo", priority: "low", label: CL_LABELS.bug },
  { id: "EXP-145", title: "Nightly inventory sync job", status: "backlog", priority: "none", label: CL_LABELS.infra },
  { id: "EXP-146", title: "Dark mode for the storefront", status: "backlog", priority: "none", label: CL_LABELS.web },
  { id: "EXP-144", title: "Fix coupon stacking on sale items", status: "done", priority: "medium", assignee: CL.initials },
  { id: "EXP-147", title: "Bump storefront to React 19", status: "done", priority: "low", assignee: CL.initials },
]

// ── EXP-151 detail-pane content ───────────────────────────────────────────────
export const CL_ISSUE = {
  id: NEW_ISSUE_ID,
  title: REPORT.title,
  descriptionParas: [
    REPORT.details,
    "Reported from acme.shop/checkout via the feedback widget · Safari 26.2 on macOS · viewport 1568×980.",
  ],
  switcher: "4 / 8",
  activity: [
    { actor: "Feedback widget", text: "created this issue" },
    { actor: CL.reporter, text: "subscribed as reporter" },
  ],
  imagesMeta: "1 image",
  pr: CL.pr,
  label: CL_LABELS.widget,
  project: CL.project,
  projectColor: CL.projectColor,
} as const

// ── Start-coding dialog checklist (EXP-151 checked, open siblings unchecked) ──
export type DialogIssueRow = { id: string; title: string; right: string; checked?: boolean }
export const DIALOG_ISSUES: DialogIssueRow[] = [
  { id: NEW_ISSUE_ID, title: REPORT.title, right: "Todo", checked: true },
  { id: "EXP-149", title: "Add Apple Pay to checkout", right: "Todo" },
  { id: "EXP-150", title: "Order confirmation email renders twice", right: "Todo" },
  { id: "EXP-148", title: "Migrate product search to Typesense", right: "In Progress" },
]

// ── The Claude session (real CLI grammar, see ships/surfaces/terminal.tsx) ────
export const CL_SESSION: SessionEvent[] = [
  { kind: "tool", tool: "Read", args: "apps/shop/src/checkout/PayButton.tsx", result: "Read 148 lines" },
  { kind: "prose", text: "The submit handler bails while cart revalidation is pending — on Safari that promise never settles, so every click is swallowed. Fixing the pending state and re-enabling submit:" },
  { kind: "tool", tool: "Update", args: "apps/shop/src/checkout/PayButton.tsx", result: "Added 18 lines, removed 6 lines" },
  { kind: "tool", tool: "Write", args: "apps/shop/src/checkout/pay-button.test.tsx", result: "Created file with 42 lines" },
  { kind: "tool", tool: "Bash", args: "bun run typecheck", result: "0 errors" },
  { kind: "tool", tool: "Bash", args: "bun test checkout", result: "6 pass · 0 fail" },
  { kind: "spinner", verb: "Vibing" },
  { kind: "tool", tool: "Bash", args: "git push -u origin exp/EXP-151", result: "To github.com:acme/shop.git" },
  { kind: "tool", tool: "mcp__exponential__exponential_pr_open" },
  { kind: "flash", text: `Opened PR #218 — ${REPORT.title}` },
]

// ── The EXP-151 diff (Changes tab) ────────────────────────────────────────────
export const CL_DIFF_HEADER = {
  branch: CL.branch,
  pr: `PR #${CL.pr}`,
  stats: { files: 2, add: 24, del: 6 },
} as const

export const CL_DIFF_FILES = [
  { status: "M", path: "apps/shop/src/checkout/PayButton.tsx", selected: true },
  { status: "A", path: "apps/shop/src/checkout/pay-button.test.tsx" },
] as const

export const CL_FILE_STATS = { add: 18, del: 6 } as const

export const CL_DIFF_ROWS: DiffRow[] = [
  { t: "hunk", text: "@@ -21,9 +21,18 @@ export function PayButton({ cart }: PayButtonProps)" },
  { t: "ctx", text: "  const checkout = useCheckout(cart)", old: 21, new: 21 },
  { t: "ctx", text: "  const [submitting, setSubmitting] = useState(false)", old: 22, new: 22 },
  { t: "ctx", text: "", old: 23, new: 23 },
  { t: "del", text: "  const revalidating = cart.revalidation.pending", old: 24 },
  { t: "add", text: "  const revalidation = useRevalidation(cart, {", new: 24 },
  { t: "add", text: "    timeoutMs: 4_000,", new: 25 },
  { t: "add", text: "    onTimeout: () => setSubmitting(false),", new: 26 },
  { t: "add", text: "  })", new: 27 },
  { t: "ctx", text: "", old: 25, new: 28 },
  { t: "del", text: "  const onPay = () => {", old: 26 },
  { t: "del", text: "    if (revalidating) return", old: 27 },
  { t: "add", text: "  const onPay = async () => {", new: 29 },
  { t: "add", text: "    if (revalidation.pending) await revalidation.settled", new: 30 },
  { t: "add", text: "    if (submitting) return", new: 31 },
  { t: "ctx", text: "    setSubmitting(true)", old: 28, new: 32 },
  { t: "ctx", text: "    checkout.submit()", old: 29, new: 33 },
  { t: "ctx", text: "  }", old: 30, new: 34 },
  { t: "hunk", text: "@@ -44,7 +53,10 @@ export function PayButton({ cart }: PayButtonProps)" },
  { t: "ctx", text: "  return (", old: 44, new: 53 },
  { t: "ctx", text: "    <button", old: 45, new: 54 },
  { t: "del", text: "      disabled={revalidating}", old: 46 },
  { t: "add", text: "      disabled={submitting}", new: 55 },
  { t: "add", text: "      aria-busy={submitting}", new: 56 },
  { t: "add", text: "      data-state={revalidation.pending ? `revalidating` : `ready`}", new: 57 },
  { t: "ctx", text: "      onClick={onPay}", old: 47, new: 58 },
  { t: "ctx", text: "      className={`pay-button`}", old: 48, new: 59 },
  { t: "ctx", text: "    >", old: 49, new: 60 },
  { t: "ctx", text: "      {submitting ? `Processing…` : `Pay now`}", old: 50, new: 61 },
]

// ── Reviews row ───────────────────────────────────────────────────────────────
export const CL_REVIEW_ROW = {
  id: NEW_ISSUE_ID,
  title: REPORT.title,
  sub: `#${CL.pr} · ${CL.branch}`,
} as const

// ── Overlay copy ──────────────────────────────────────────────────────────────
export const COPY = {
  s1: "A visitor hits a bug.",
  s2: "They report it in place.",
  s3: "It lands on your board.",
  s5: "Start coding.",
  s6: "Claude fixes it in the dock.",
  s7: "Review it in place.",
  s8: "Merge. Done.",
} as const

// ── The Shipped end-card + platform lineup (S9, EXP-200) ─────────────────────
export const ENDING_COPY = {
  title: "Shipped.",
  sub: "The loop closes.",
} as const
