import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { isGroupChatRoot } from "@/lib/group-chat"
import { useNavigate } from "@tanstack/react-router"
import { AppHighlightText, conceptIcon, type PickerItem } from "@exp/ui"
import type { ChatHit } from "@exp/domain-contract/chat-store"
import { findInChatQuery } from "@exp/domain-contract/find-in-chat"
import { globalChatPick, globalChatRows, SEARCH_LIST_CHATS } from "@exp/domain-contract/search-list"
import { chatStoreCall } from "@/lib/chat-store/client"
import { sessionCollection } from "@/lib/collections"
import { sessionTarget } from "@/lib/session-back"
import type { SessionRow } from "@/lib/session-tree"
import { useSession } from "@/hooks/use-session"
import { useTeamChatTitles, useTeamSessions } from "@/hooks/use-session-tree"
import { localUtcOffsetMinutes } from "@/hooks/use-chat-list"
import { seedFindInChat } from "@/components/chat/find-in-chat"
import type { GlobalChatsSection } from "@/components/issue-picker-host"

// #EDE-99 (search-list.json CHATS): the global search's Chats section ×web.
// The query goes to the chat store's worker (`searchAll` over the log per
// chat, off the UI thread) `debounceMs` after the last keystroke, only for
// the ACTIVE team's synced roots; a newer query drops the older answer and
// the rows of the previous one stay (the band's spinner) until it lands.
// A pick seeds find in chat of the session it opens (its hit's stop).

type StoreChat = { rootId: string; count: number; hit: ChatHit }
type Answer = { query: string; chats: StoreChat[] }

const ICONS = {
  [SEARCH_LIST_CHATS.icons.chat]: conceptIcon(`search-chat`),
  [SEARCH_LIST_CHATS.icons.agent]: conceptIcon(`search-agent`),
}

export function useGlobalChatSearch(input: {
  /** #EXP-2027: every team searched (md+: every member team). */
  teams: readonly { id: string; slug: string }[]
  query: string
  open: boolean
}): GlobalChatsSection | null {
  const { teams, open } = input
  const navigate = useNavigate()
  const { data: auth } = useSession()
  const viewerId = auth?.user?.id ?? null
  const teamIds = useMemo(() => teams.map((team) => team.id), [teams])
  const slugOf = useMemo(() => new Map(teams.map((team) => [team.id, team.slug])), [teams])
  const { rows } = useTeamSessions(open ? teamIds : null)
  const roots = useMemo(() => rows.filter((row) => !row.parentId), [rows])
  // The Chats list's titles (team-wide issue links and digests).
  const titles = useTeamChatTitles(open ? teamIds : null)
  const rootIds = useMemo(() => roots.map((row) => row.id).sort(), [roots])
  const rootKey = rootIds.join(`,`)

  // null = no store on this client (today's list); undefined = not known yet.
  const [available, setAvailable] = useState<boolean | undefined>(undefined)
  const [answer, setAnswer] = useState<Answer | null>(null)
  const [asked, setAsked] = useState(``)
  const query = findInChatQuery(input.query)
  // Whether this browser keeps a store at all (asked once per opening).
  useEffect(() => {
    if (!open || available !== undefined) return
    let live = true
    void chatStoreCall(`constants`)
      .catch(() => undefined)
      .then((result) => {
        if (live) setAvailable(result !== undefined)
      })
    return () => {
      live = false
    }
  }, [open, available])
  useEffect(() => {
    if (!open || query === `` || rootIds.length === 0) {
      setAsked(``)
      return
    }
    let live = true
    const timer = window.setTimeout(() => {
      setAsked(query)
      void chatStoreCall<{ chats: StoreChat[] } | null>(`searchAll`, rootIds, query, SEARCH_LIST_CHATS.chatsMax)
        .catch(() => undefined)
        .then((result) => {
          if (!live) return
          if (result === undefined) {
            setAvailable(false)
            return
          }
          setAvailable(true)
          // null = a newer search ended this one in the worker.
          if (result) setAnswer({ query, chats: result.chats })
        })
    }, SEARCH_LIST_CHATS.debounceMs)
    return () => {
      live = false
      window.clearTimeout(timer)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, query, rootKey])
  useEffect(() => {
    if (!open) setAnswer(null)
  }, [open])

  const rootsById = useMemo(() => {
    const out: Record<string, { title: string; agent: boolean }> = {}
    for (const row of roots) out[row.id] = { title: titles.get(row.id) ?? row.title ?? ``, agent: Boolean(row.agentId) }
    return out
  }, [roots, titles])
  const answerChats = answer?.chats
  const answerQuery = answer?.query ?? ``
  const items = useMemo<PickerItem<string>[]>(() => {
    if (!answerChats || query === ``) return []
    const now = new Date()
    return globalChatRows({
      query: answerQuery,
      now: now.toISOString(),
      utcOffsetMinutes: localUtcOffsetMinutes(now),
      chats: answerChats,
      roots: rootsById,
      messageAuthors: {},
      limit: SEARCH_LIST_CHATS.chatsMax,
    }).map((row) => {
      const Icon = ICONS[row.icon]
      return {
        value: row.key,
        label: row.title,
        icon: Icon,
        description: (
          <span className="block min-w-0 truncate">
            <AppHighlightText text={row.subtitle} highlight={{ query: row.highlight }} />
          </span>
        ),
        hint: row.trailing,
      }
    })
  }, [answerChats, answerQuery, rootsById, query])

  const chatsRef = useRef(answerChats)
  chatsRef.current = answerChats
  const onPick = useCallback(
    (value: string) => {
      const chat = chatsRef.current?.find((entry) => `chat:${entry.rootId}` === value)
      if (!chat) return
      const pick = globalChatPick({ query: answerQuery, hit: chat.hit })
      seedFindInChat(pick.sessionId, { query: pick.find.query, ...pick.find.target })
      const target = sessionTarget(pick.sessionId, (id) => sessionCollection.get(id) as SessionRow | undefined, viewerId, isGroupChatRoot)
      // The chat's own team (#EXP-2027: the search spans every member team).
      const teamSlug = slugOf.get((sessionCollection.get(chat.rootId) as SessionRow | undefined)?.teamId ?? ``) ?? teams[0]?.slug
      if (!teamSlug) return
      if (target.to === `agent`) {
        void navigate({ to: `/t/$teamSlug/agents/$agentId`, params: { teamSlug, agentId: target.agentId } })
      } else if (target.to === `chat`) {
        void navigate({
          to: `/t/$teamSlug/sessions/$sessionId`,
          params: { teamSlug, sessionId: target.chatId },
          search: target.session ? { tab: `sessions` as const, open: `session` as const, session: target.session } : {},
        })
      } else {
        void navigate({ to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug, sessionId: target.sessionId } })
      }
    },
    [answerQuery, navigate, slugOf, teams, viewerId]
  )

  if (available !== true || !open) return null
  const loading = query !== `` && (asked !== query || answerQuery !== query)
  return { items, loading, onPick }
}
