// closedloop/fixtures.ts — the ONE fixture world of the ClosedLoop film
// (EXP-337 five per-flow clips, reordered by EXP-385, calmed by EXP-388):
// the team works the Acme Shop board in realtime, a coding run on EXP-151
// starts FROM THE PHONE and spawns in the desktop dock where it's steered
// live, the PR merges from the Reviews row, a visitor reports the bug
// through the embedded feedback widget, and the platform lineup closes the
// loop — which wraps back into the live board when it restarts. Everything
// here is deterministic copy — no divergent content.

import type {
  BoardRow,
  DiffRow,
  SessionEvent,
  SteerItem,
} from "../ships/fixtures"

// ── Identity (the team whose product is acme.shop) ────────────────────────────
export const CL = {
  siteUrl: "acme.shop/checkout",
  brand: "ACME SHOP",
  reporter: "Jamie Lee",
  reporterEmail: "jamie@lee.dev",
  project: "Acme Shop",
  team: "Acme", // the rail header names the ACTIVE team (EXP-723)
  projectColor: "#a1a1aa",
  repo: "acme/shop",
  branch: "exp/EXP-151",
  pr: 218,
  runConfig: "Storefront",
  user: "Riley Chen",
  initials: "RC",
  sessionTab: "Fix the dead checkout button",
} as const

// The rail's Boards group (icons.json pickable glyphs + their accents).
export const CL_BOARDS = [
  { name: CL.project, glyph: "code", color: "#818cf8" },
  { name: "Launch Marketing", glyph: "kanban", color: "#f59e0b" },
  { name: "Product Feedback", glyph: "megaphone", color: "#22c55e" },
] as const

export const CL_LABELS = {
  bug: { name: "bug", dot: "#ef4444" },
  web: { name: "web", dot: "#f97316" },
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
    {
      name: "Fjord Parka",
      variant: "Slate · M",
      price: "$148.00",
      tint: "#94a3b8",
    },
    {
      name: "Trail Socks",
      variant: "2 pairs",
      price: "$24.00",
      tint: "#d6b88a",
    },
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
  sentTitle: "Feedback sent",
  titleLabel: "Title",
  title: "Checkout button does nothing",
  detailsLabel: "Details",
  details: "Clicked “Pay now” on Safari. No response.",
  send: "Send feedback",
  sending: "Sending…",
  successTitle: "Thanks for the report!",
  successSub: "Filed as ",
  successId: "EXP-151",
  poweredBy: "Powered by Exponential",
} as const

// ── Board at story start (EXP-151 pops in via insertAt) ──────────────────────
export const NEW_ISSUE_ID = "EXP-151"

export const CL_BOARD: BoardRow[] = [
  {
    id: "EXP-148",
    title: "Migrate product search to Typesense",
    status: "in_progress",
    priority: "high",
    label: CL_LABELS.infra,
    assignee: CL.initials,
    due: "Jul 18",
  },
  {
    id: NEW_ISSUE_ID,
    title: REPORT.title,
    status: "backlog",
    priority: "none",
    label: CL_LABELS.widget,
    assignee: CL.initials,
    due: "Jul 19",
  },
  {
    id: "EXP-149",
    title: "Add Apple Pay to checkout",
    status: "backlog",
    priority: "medium",
    label: CL_LABELS.web,
    assignee: CL.initials,
    due: "Jul 21",
  },
  {
    id: "EXP-150",
    title: "Order confirmation email renders twice",
    status: "backlog",
    priority: "low",
    label: CL_LABELS.bug,
  },
  {
    id: "EXP-145",
    title: "Nightly inventory sync job",
    status: "backlog",
    priority: "none",
    label: CL_LABELS.infra,
  },
  {
    id: "EXP-146",
    title: "Dark mode for the storefront",
    status: "backlog",
    priority: "none",
    label: CL_LABELS.web,
  },
  {
    id: "EXP-144",
    title: "Fix coupon stacking on sale items",
    status: "done",
    priority: "medium",
    label: CL_LABELS.web,
    assignee: CL.initials,
  },
  {
    id: "EXP-147",
    title: "Bump storefront to React 19",
    status: "done",
    priority: "low",
    label: CL_LABELS.infra,
    assignee: CL.initials,
  },
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
    { actor: "Feedback widget", text: "created this issue", time: "1 hr ago" },
    { actor: CL.reporter, text: "subscribed as reporter", time: "1 hr ago" },
  ],
  comments: [
    {
      actor: CL.reporter,
      initials: "JL",
      time: "12 min ago",
      body: "Happens every time on Safari 26.2 — the button just does not respond. Chrome is fine.",
    },
  ],
  pr: CL.pr,
  label: CL_LABELS.widget,
  assigneeName: CL.user,
  due: "Jul 19",
  project: CL.project,
  projectColor: "#818cf8",
  // EXP-496: a widget-filed issue carries an Origin chip on every client.
  origin: "Feedback widget",
} as const

// ── The phone start flow (remote start over the steer rails, EXP-385) ────────
// Strings mirror the real StartCodingSheet (EXP-687 chrome: grabber on top, no
// toolbar buttons, one pinned full-width Start coding button at the floor), an
// Issues section with a search row, the agent pill strip, and Model + Effort
// picker rows. One desktop online = no Device row at all;
// after submit the caller shows the "Start sent" capsule toast.
export const PHONE_START = {
  cancel: "Cancel",
  confirm: "Start coding",
  issuesLabel: "Issues",
  searchPlaceholder: "Search issues",
  modelLabel: "Model",
  model: "Fable",
  effortLabel: "Effort",
  effort: "CLI default",
  device: "MacBook Pro",
  toast: "Start sent to MacBook Pro. Watch it in the Agents tab.",
} as const

// ── The Claude session (real CLI grammar, see ships/surfaces/terminal.tsx) ────
export const CL_SESSION: SessionEvent[] = [
  {
    kind: "tool",
    tool: "Read",
    args: "apps/shop/src/checkout/PayButton.tsx",
    result: "Read 148 lines",
  },
  {
    kind: "prose",
    text: "The submit handler bails while cart revalidation is pending. On Safari that promise never settles, so every click is swallowed. Fixing the pending state and re-enabling submit:",
  },
  {
    kind: "tool",
    tool: "Update",
    args: "apps/shop/src/checkout/PayButton.tsx",
    result: "Added 18 lines, removed 6 lines",
  },
  {
    kind: "tool",
    tool: "Write",
    args: "apps/shop/src/checkout/pay-button.test.tsx",
    result: "Created file with 42 lines",
  },
  { kind: "tool", tool: "Bash", args: "bun run typecheck", result: "0 errors" },
  {
    kind: "tool",
    tool: "Bash",
    args: "bun test checkout",
    result: "6 pass · 0 fail",
  },
  { kind: "spinner", verb: "Vibing" },
  {
    kind: "tool",
    tool: "Bash",
    args: "git push -u origin exp/EXP-151",
    result: "To github.com:acme/shop.git",
  },
  { kind: "tool", tool: "mcp__exponential__exponential_pr_open" },
  { kind: "flash", text: `Opened PR #218: ${REPORT.title}` },
]

// ── The EXP-151 diff (the PrDiff center screen Reviews rows open) ────────────
export const CL_PR_HEAD = {
  identifier: NEW_ISSUE_ID,
  title: REPORT.title,
  pr: CL.pr,
  branch: CL.branch,
} as const

export const CL_DIFF_FILES = [
  { status: "M", path: "apps/shop/src/checkout/PayButton.tsx", selected: true },
  {
    status: "A",
    path: "apps/shop/src/checkout/pay-button.test.tsx",
    add: 42,
    del: 0,
  },
] as const

export const CL_FILE_STATS = { add: 18, del: 6 } as const

export const CL_DIFF_ROWS: DiffRow[] = [
  {
    t: "hunk",
    text: "@@ -21,9 +21,18 @@ export function PayButton({ cart }: PayButtonProps)",
  },
  { t: "ctx", text: "  const checkout = useCheckout(cart)", old: 21, new: 21 },
  {
    t: "ctx",
    text: "  const [submitting, setSubmitting] = useState(false)",
    old: 22,
    new: 22,
  },
  { t: "ctx", text: "", old: 23, new: 23 },
  {
    t: "del",
    text: "  const revalidating = cart.revalidation.pending",
    old: 24,
  },
  { t: "add", text: "  const revalidation = useRevalidation(cart, {", new: 24 },
  { t: "add", text: "    timeoutMs: 4_000,", new: 25 },
  { t: "add", text: "    onTimeout: () => setSubmitting(false),", new: 26 },
  { t: "add", text: "  })", new: 27 },
  { t: "ctx", text: "", old: 25, new: 28 },
  { t: "del", text: "  const onPay = () => {", old: 26 },
  { t: "del", text: "    if (revalidating) return", old: 27 },
  { t: "add", text: "  const onPay = async () => {", new: 29 },
  {
    t: "add",
    text: "    if (revalidation.pending) await revalidation.settled",
    new: 30,
  },
  { t: "add", text: "    if (submitting) return", new: 31 },
  { t: "ctx", text: "    setSubmitting(true)", old: 28, new: 32 },
  { t: "ctx", text: "    checkout.submit()", old: 29, new: 33 },
  { t: "ctx", text: "  }", old: 30, new: 34 },
  {
    t: "hunk",
    text: "@@ -44,7 +53,10 @@ export function PayButton({ cart }: PayButtonProps)",
  },
  { t: "ctx", text: "  return (", old: 44, new: 53 },
  { t: "ctx", text: "    <button", old: 45, new: 54 },
  { t: "del", text: "      disabled={revalidating}", old: 46 },
  { t: "add", text: "      disabled={submitting}", new: 55 },
  { t: "add", text: "      aria-busy={submitting}", new: 56 },
  {
    t: "add",
    text: "      data-state={revalidation.pending ? `revalidating` : `ready`}",
    new: 57,
  },
  { t: "ctx", text: "      onClick={onPay}", old: 47, new: 58 },
  { t: "ctx", text: "      className={`pay-button`}", old: 48, new: 59 },
  { t: "ctx", text: "    >", old: 49, new: 60 },
  {
    t: "ctx",
    text: "      {submitting ? `Processing…` : `Pay now`}",
    old: 50,
    new: 61,
  },
]

// ── Reviews row ───────────────────────────────────────────────────────────────
export const CL_REVIEW_ROW = {
  id: NEW_ISSUE_ID,
  title: REPORT.title,
  sub: `#${CL.pr} · ${CL.branch}`,
} as const

// ── The live steer (inside the code-everywhere clip) ─────────────────────────
// The dock streams the session while the SAME feed mirrors onto the phone's
// steer activity view; the user types a steer on the phone, it lands in the
// terminal as a highlighted line, and the agent acknowledges and continues.
export const CL_STEER_MSG =
  "Also guard double-submits: disable the button while a payment is in flight."

export const CL_STEER_REPLY: SessionEvent[] = [
  {
    kind: "prose",
    text: "Good catch. Adding an in-flight guard and a double-submit test.",
  },
  {
    kind: "tool",
    tool: "Update",
    args: "apps/shop/src/checkout/PayButton.tsx",
    result: "Added 9 lines",
  },
  {
    kind: "tool",
    tool: "Bash",
    args: "bun test checkout",
    result: "7 pass · 0 fail",
  },
]

// The phone's steer activity feed (mirrors the session, condensed).
export const CL_PHONE_FEED: SteerItem[] = [
  { kind: "tool", name: "Read", summary: "PayButton.tsx" },
  {
    kind: "narration",
    text: "The submit handler bails while cart revalidation is pending. Fixing the pending state and re-enabling submit:",
  },
  {
    kind: "tool",
    name: "Update",
    summary: "apps/shop/src/checkout/PayButton…",
  },
]

// ── The board-live clip: a teammate's change lands live, then a push ────────
// (The product ships no presence facepile and no remote cursors — there is no
// presence shape — so the clip shows only what actually syncs.)
export const REMOTE_DRAG_ID = "EXP-149" // Mara drags "Add Apple Pay" Backlog → In Progress
export const LIVE_EDIT_ID = "EXP-150" // a teammate edit flashes in live

// Pushes land with the REAL notification grammar (lib/integrations/
// notifications.ts): title "{Actor} changed {ID} to {Status}" / "New feedback:
// {ID}", body = the issue title.
export const PUSH_NOTIFICATION = {
  title: `Mara changed ${REMOTE_DRAG_ID} to In Progress`,
  body: "Add Apple Pay to checkout",
} as const

export const PUSH_FEEDBACK = {
  title: `New feedback: ${NEW_ISSUE_ID}`,
  body: REPORT.title,
} as const

// The mobile board list (BoardScreen) — the same CL_BOARD world projected
// onto the real IssueListView row shape (label → dot, assignee → initial).
export type PhoneBoardProjection = {
  id: string
  title: string
  priority: "none" | "urgent" | "high" | "medium" | "low"
  status: "backlog" | "in_progress" | "done"
  labelDot?: string
  assignee?: string
  due?: string
}

export const CL_PHONE_BOARD: PhoneBoardProjection[] = CL_BOARD.map((row) => ({
  id: row.id,
  title: row.title,
  priority: row.priority,
  status: row.status,
  labelDot: row.label?.dot,
  assignee: row.assignee,
  due: row.due,
}))

// ── The platform-lineup finale ───────────────────────────────────────────────
export const PLATFORMS_COPY = {
  title: "Exponential",
  sub: "Go exponential.",
} as const

// ── Caption copy (screen-space, up to three per clip) ────────────────────────
export const COPY = {
  bl1: "Your whole team, live on one board.",
  bl2: "Every change, pushed to your phone.",
  ce1: "Start coding from anywhere.",
  ce2: "It runs on your own machine.",
  ce3: "Steer it live.",
  rm1: "Review it in place.",
  rm2: "Merge. Done.",
  fb1: "A visitor hits a bug.",
  fb2: "It lands on your board.",
} as const
