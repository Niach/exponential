/* The eApps concept pictures (exponential.at/apps, docs/spec/apps.md): eUI
   trees painted by the React renderer's harness. `capture.ts` renders them
   into apps/marketing/public/eapps/ and docs/spec/apps/. Illustrative data
   only: neutral names, English copy. */

type Node = { id: string; component: string; props?: Record<string, unknown>; children?: Node[]; slots?: Record<string, Node>; style?: Record<string, unknown> }

export type Mock = { file: string; width: number; tree: Node }

let n = 0
const id = (p: string) => `${p}-${++n}`
const node = (component: string, props: Record<string, unknown> = {}, children?: Node[], extra: Partial<Node> = {}): Node => ({ id: id(component.toLowerCase()), component, props, ...(children ? { children } : {}), ...extra })
const stack = (props: Record<string, unknown>, children: Node[], style?: Record<string, unknown>) => node(`Stack`, props, children, style ? { style } : {})
const text = (t: string, props: Record<string, unknown> = {}) => node(`Text`, { text: t, ...props })
const heading = (t: string, level = `h3`) => node(`Heading`, { text: t, level })
const button = (label: string, props: Record<string, unknown> = {}) => node(`Button`, { label, ...props })

/* ── Hours: a time tracker ─────────────────────────────────────────── */

const DAYS = [1, 2, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 19, 20, 21, 22, 23, 26, 27, 28, 29, 30]
const HOURS = [8, 7.5, 6, 8, 8.5, 8, 7, 4, 8, 9, 8, 7.5, 6, 8, 8, 7.5, 8, 5, 8, 6.5, 0, 0]

const ENTRIES = [
  { date: `2026-10-28`, hours: 6.5, project: `Exponential`, activity: `eApps spec, record ownership`, issues: `VAPP-175` },
  { date: `2026-10-27`, hours: 8, project: `Exponential`, activity: `eMCP gate and audit`, issues: `EXP-1490, EXP-1491` },
  { date: `2026-10-26`, hours: 5, project: `Acme`, activity: `Billing: Stripe webhooks`, issues: `ACME-41` },
  { date: `2026-10-23`, hours: 8, project: `Exponential`, activity: `Carousel ×6`, issues: `EXP-1493` },
  { date: `2026-10-22`, hours: 7.5, project: `Spezl`, activity: `Loopback MCP`, issues: `YO-32` },
]

const header = (title: string, subtitle: string) => node(`AppBar`, { title, subtitle })

export const TIME_TRACKER: Mock = {
  file: `time-tracker`,
  width: 1180,
  tree: stack({ gap: `none` }, [
    header(`Hours`, `October 2026 · Mia Keller`),
    stack({ gap: `lg` }, [
      stack({ direction: `horizontal`, align: `center`, justify: `between` }, [
        stack({ direction: `horizontal`, gap: `sm`, align: `center` }, [
          button(``, { icon: `chevron-left`, variant: `outline`, size: `icon` }),
          text(`October 2026`, { variant: `label` }),
          button(``, { icon: `chevron-right`, variant: `outline`, size: `icon` }),
          node(`Select`, { name: `scope`, value: `mine`, options: [{ label: `My hours`, value: `mine` }, { label: `Everyone`, value: `all` }] }),
        ]),
        button(`Log hours`, { icon: `plus`, variant: `default` }),
      ]),
      node(`Card`, {}, [
        node(`Chart`, { kind: `bar`, title: `Per day`, height: 220, categories: DAYS.map(String), series: [{ name: `Hours`, values: HOURS, tone: `info` }], max: 10, showAxes: true, showGrid: true }),
      ]),
      node(`Table`, {
        columns: [
          { key: `date`, label: `Date`, type: `date`, width: 150 },
          { key: `hours`, label: `Hours`, type: `number`, decimals: 1, align: `end`, width: 80 },
          { key: `project`, label: `Project`, type: `badge`, width: 150 },
          { key: `activity`, label: `Activity` },
          { key: `issues`, label: `Issues`, width: 190 },
        ],
        rows: ENTRIES,
        rowKey: `date`,
      }),
    ], { padding: `24px` }),
  ]),
}

export const TIME_TRACKER_PHONE: Mock = {
  file: `time-tracker-phone`,
  width: 390,
  tree: stack({ gap: `none` }, [
    header(`Hours`, `October 2026`),
    stack({ gap: `lg` }, [
      node(`List`, { divided: false, gap: `none` }, ENTRIES.slice(0, 4).map((e) =>
        node(`Row`, { title: new Date(`${e.date}T12:00:00Z`).toLocaleDateString(`en-US`, { weekday: `short`, month: `short`, day: `numeric`, timeZone: `UTC` }), subtitle: e.activity, value: `${e.hours.toFixed(1)} h`, chevron: `right`, surface: `card` })
      )),
      button(`Log hours`, { icon: `plus`, variant: `default`, size: `lg` }),
    ], { padding: `20px` }),
  ]),
}

/* ── Admin console: one screen over several upstreams ──────────────── */

const navRow = (title: string, icon: string, extra: Record<string, unknown> = {}) => node(`Row`, { title, icon, density: `compact`, ...extra })

export const ADMIN_CONSOLE: Mock = {
  file: `admin-console`,
  width: 1280,
  tree: node(`Sidebar`, { width: 180, collapseBelow: `never` }, [
    stack({ gap: `none` }, [
      header(`Admin console`, `acme.example · live`),
      stack({ gap: `lg` }, [
        node(`Grid`, { columns: 4, gap: `md` }, [
          node(`StatTile`, { label: `MRR`, value: `€18,420`, delta: `+4.2%`, tone: `success` }),
          node(`StatTile`, { label: `Active subscriptions`, value: `389`, delta: `+11`, tone: `success` }),
          node(`StatTile`, { label: `Failed payments`, value: `7`, delta: `€612 at risk`, tone: `danger` }),
          node(`StatTile`, { label: `Open PRs`, value: `5`, delta: `2 need review`, tone: `info` }),
        ]),
        node(`Alert`, { type: `warning`, title: `Home Assistant token expired`, message: `Reconnect to control the office again. Only your own connection is affected.` }),
        node(`Card`, { title: `Failed payments` }, [
          node(`Table`, {
            columns: [
              { key: `customer`, label: `Customer` },
              { key: `plan`, label: `Plan`, type: `badge`, width: 110 },
              { key: `amount`, label: `Amount`, type: `currency`, currency: `EUR`, align: `end`, width: 110 },
              { key: `since`, label: `Since`, type: `date`, width: 130 },
              { key: `status`, label: `Stripe`, type: `badge`, width: 110 },
            ],
            rows: [
              { id: `c1`, customer: `Northside Physio`, plan: `Clinic`, amount: 89, since: `2026-10-29`, status: `past_due` },
              { id: `c2`, customer: `Lind Family Practice`, plan: `Clinic+`, amount: 149, since: `2026-10-27`, status: `past_due` },
              { id: `c3`, customer: `Ring Road Therapy`, plan: `Solo`, amount: 39, since: `2026-10-21`, status: `unpaid` },
            ],
            rowKey: `id`,
            selectable: `multiple`,
            selected: [`c1`],
          }),
        ]),
        stack({ direction: `horizontal`, align: `center`, justify: `between` }, [
          text(`1 selected`, { variant: `muted` }),
          stack({ direction: `horizontal`, gap: `sm` }, [button(`Retry charge`, { icon: `refresh-cw`, variant: `outline` }), button(`Ask agent to follow up`, { icon: `sparkles`, variant: `default` })]),
        ]),
      ], { padding: `20px` }),
    ]),
  ], {
    slots: {
      sidebar: stack({ gap: `xs` }, [
        navRow(`Overview`, `layout-dashboard`, { selected: true }),
        navRow(`Customers`, `users`, { meta: `412` }),
        navRow(`Subscriptions`, `credit-card`),
        navRow(`Releases`, `github`),
        navRow(`Office`, `house`),
        node(`Separator`),
        node(`Row`, { title: `Stripe`, subtitle: `connected as mia@`, icon: `plug` }),
        node(`Row`, { title: `GitHub`, subtitle: `connected`, icon: `plug` }),
        node(`Row`, { title: `Home Assistant`, subtitle: `Reconnect`, icon: `plug` }),
      ]),
    },
  }),
}

/* ── The MCP consent: a client asks for the team's apps ───────────── */

const ACCESS = [{ label: `Off`, value: `off` }, { label: `Read only`, value: `read` }, { label: `Full`, value: `full` }]
const appRow = (title: string, icon: string, tools: string, value: string) =>
  node(`Row`, { title, subtitle: tools, icon }, undefined, { slots: { trailing: node(`Segmented`, { items: ACCESS, value, size: `sm` }) } })

export const MCP_CONSENT: Mock = {
  file: `mcp-consent`,
  width: 620,
  tree: stack({ gap: `lg` }, [
    stack({ gap: `xs` }, [heading(`Claude Code wants to use your apps`, `h2`), text(`Team Acme · acting as mia@acme.example · every call is audited`, { variant: `muted` })]),
    node(`Card`, { title: `Apps` }, [
      appRow(`Hours`, `clock`, `log_hours · month_summary · export_month`, `full`),
      appRow(`Admin console`, `briefcase`, `failed_payments · retry_charge · open_prs`, `read`),
      appRow(`CRM`, `contact`, `find_contact · log_call`, `off`),
    ]),
    node(`Card`, { title: `Upstreams Admin console needs` }, [
      node(`Row`, { title: `Stripe`, subtitle: `Connected`, icon: `circle-check` }),
      node(`Row`, { title: `GitHub`, subtitle: `Connected`, icon: `circle-check` }),
      node(`Row`, { title: `Customer MCP`, subtitle: `Not connected`, icon: `plug` }, undefined, { slots: { trailing: button(`Connect`, { variant: `outline`, size: `sm` }) } }),
    ]),
    node(`Alert`, { type: `info`, title: `Destructive tools are never exposed`, message: `Tools run as you, read “via <app>”, and see only their own app's records and upstreams.` }),
    stack({ direction: `horizontal`, gap: `sm`, justify: `end` }, [button(`Cancel`, { variant: `outline` }), button(`Allow`, { variant: `default` })]),
  ], { padding: `20px` }),
}

export const MOCKS = [TIME_TRACKER, TIME_TRACKER_PHONE, ADMIN_CONSOLE, MCP_CONSENT]
