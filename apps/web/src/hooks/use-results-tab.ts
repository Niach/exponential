import { useCallback, useMemo } from "react"
import { and, eq, isNull, not, useLiveQuery } from "@tanstack/react-db"
import { isResultCard, resultsTab, type Result, type ResultsMessageInput } from "@exp/domain-contract/results-tab"
import { currentKeySource } from "@/lib/enet/current-key-holder"
import { openMessageWith } from "@/lib/enet/sealed-open"
import { useKeyVersion } from "@/lib/enet/use-key-holder"
import { messageCollection } from "@/lib/collections"
import { sharedLiveQuery } from "@/lib/shared-live-query"
import { useNow } from "@/hooks/use-now"
import { useSessionTitles, useTeamSessions } from "@/hooks/use-session-tree"

// #FEED-191 RESULTS TAB (fixtures/results-tab.json): every result of the root's
// tree (#EXP-1380: its session-authored card rows, narrowed by rootId; sealed
// cards opened, a locked one is never a result), folded by `resultsTab()`.

type LoadedCardRow = {
  id: string
  rootId: string
  // #EXP-1730: the session the row lives in (null = a dm/channel row).
  sessionId: string | null
  authorSessionId: string | null
  createdAt: unknown
  card: unknown
}

const ms = (value: unknown): number => {
  const at = value instanceof Date ? value.getTime() : new Date(value as string | number).getTime()
  return Number.isNaN(at) ? 0 : at
}

function openedCard(row: LoadedCardRow): unknown {
  const card = row.card as Record<string, unknown> | null
  if (!card || !(`sealed` in card)) return card
  const opened = openMessageWith({ body: ``, card }, row.rootId, currentKeySource())
  return opened.locked ? null : opened.card
}

export interface ResultsTabState {
  results: Result[]
  /** A result's raw card (its surface). */
  cardFor: (result: Result) => unknown
  /** The fold is still loading (no empty state yet). */
  loading: boolean
}

export function useResultsTab(teamId: string | null | undefined, rootId: string | null): ResultsTabState {
  // #PERF-7: shared, so reopening the chat reuses the loaded subset.
  const { data, isLoading } = useLiveQuery(
    () =>
      rootId
        ? sharedLiveQuery(`results:${rootId}`, (query) =>
            query
              .from({ messages: messageCollection })
              .where(({ messages }) =>
                and(eq(messages.rootId, rootId), not(isNull(messages.authorSessionId)), not(isNull(messages.card)))
              )
          )
        : undefined,
    [rootId]
  )
  const keyVersion = useKeyVersion()
  const messages = useMemo(() => {
    const out: ResultsMessageInput[] = []
    for (const row of (data ?? []) as LoadedCardRow[]) {
      const card = openedCard(row)
      if (!isResultCard(card)) continue
      const sessionId = row.sessionId
      if (!sessionId) continue
      out.push({
        id: row.id,
        sessionId,
        authorSessionId: row.authorSessionId ?? null,
        createdAt: ms(row.createdAt),
        card,
      })
    }
    return out
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data, keyVersion])

  const { rows } = useTeamSessions(teamId ?? undefined)
  const tree = useMemo(() => rows.filter((row) => (row.rootId ?? row.id) === rootId), [rows, rootId])
  const { titles } = useSessionTitles(tree)
  const now = useNow(60_000).getTime()
  const results = useMemo(
    () =>
      rootId
        ? resultsTab({
            now,
            rootId,
            sessions: tree.map((row) => ({
              id: row.id,
              rootId: row.rootId ?? row.id,
              name: titles.get(row.id) ?? row.title ?? ``,
            })),
            messages,
          })
        : [],
    [now, rootId, tree, titles, messages]
  )
  const cards = useMemo(() => new Map(messages.map((message) => [message.id, message.card])), [messages])
  const cardFor = useCallback((result: Result) => cards.get(result.id) ?? null, [cards])
  return { results, cardFor, loading: isLoading }
}
