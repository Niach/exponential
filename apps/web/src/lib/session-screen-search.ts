import type { SessionScreenOpen, SessionScreenTab } from "@exp/domain-contract/session-screen"
import { backSearch } from "@/lib/session-back"

// #EXP-1732 (session-screen.json): THE session screen's route search (the
// chat's `/sessions/$id` and an agent's `/agents/$id`): `tab` = the active
// tab, `open` = the item opened over it (an issue, a session, a plan,
// settings) with its id. Legacy `?tab=` values keep working.

export type ChatSearch = {
  /** The tab (`sessionScreen` maps the legacy ids through `legacyTabs`). */
  tab?: SessionScreenTab | `settings`
  /** The item opened over the tab. */
  open?: SessionScreenOpen
  /** #EXP-1662: the plan card a row opened (`open=plan`). */
  plan?: string
  /** The issue open in place (`open=issue`). */
  issue?: string
  /** The session open in place (`open=session`). */
  session?: string
  /** The Results tab's review pane (`tab=results`, the complete diff). */
  section?: `all`
  /** The file in focus on the review pane. */
  file?: string
  /** #FEED-191 (needs-you.json): the chat scrolls to this card, THE
   *  waiting chip's way, once it is there. */
  card?: string
  /** Where it was opened from (`TABLESS_ORIGIN` = a Running row: no tab). */
  from?: string
  /** S-3: the inter-session back stack (`lib/session-back.ts`); the
   *  header's `‹` pops it. */
  back?: string
}

const SCREEN_TABS: readonly string[] = [`chat`, `issues`, `sessions`, `results`, `preview`, `data`, `versions`, `settings`]
const SCREEN_OPENS: readonly string[] = [`issue`, `session`, `plan`, `settings`]

/** The search THE session screen reads (the chat's, an agent's). Legacy
 *  `?tab=` values land on their successor: `issue&issue=` = Issues with
 *  that issue open, `session&session=` = Sessions with it open, `plan` =
 *  the plan open, `guide`/`stories` = Results, `conversation` = Chat. */
export function validateChatSearch(search: Record<string, unknown>): ChatSearch {
  const pick = (value: unknown) => (typeof value === `string` && value ? value : undefined)
  const raw = pick(search.tab)
  let open = SCREEN_OPENS.includes(pick(search.open) ?? ``) ? (search.open as SessionScreenOpen) : undefined
  let tab: string | undefined = raw
  if (raw === `issue`) {
    tab = `issues`
    if (pick(search.issue)) open ??= `issue`
  } else if (raw === `session`) {
    tab = `sessions`
    if (pick(search.session)) open ??= `session`
  } else if (raw === `plan`) {
    tab = undefined
    open ??= `plan`
  } else if (raw === `guide` || raw === `stories`) {
    tab = `results`
  } else if (raw === `conversation`) {
    tab = `chat`
  }
  const validTab = tab && SCREEN_TABS.includes(tab) ? (tab as ChatSearch[`tab`]) : undefined
  const section = validTab === `results` && search.section === `all` ? (`all` as const) : undefined
  return {
    ...(validTab ? { tab: validTab } : {}),
    ...(open ? { open } : {}),
    ...(section ? { section } : {}),
    ...(section && pick(search.file) ? { file: pick(search.file) } : {}),
    ...(open === `issue` && pick(search.issue) ? { issue: pick(search.issue) } : {}),
    ...(open === `plan` && pick(search.plan) ? { plan: pick(search.plan) } : {}),
    ...(open === `session` && pick(search.session) ? { session: pick(search.session) } : {}),
    ...(pick(search.card) ? { card: pick(search.card) } : {}),
    ...(pick(search.from) ? { from: pick(search.from) } : {}),
    ...backSearch(search.back),
  }
}

