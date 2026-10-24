import { useCallback, useMemo, useRef, useState, type ReactNode, type RefObject } from "react"
import { SESSIONS_SEND_TEXT_MAX } from "@exp/db-schema/domain"
import {
  AttachmentFileTile,
  AttachmentThumb,
  BoardGlyph,
  CHAT_BUBBLE_ENTER_CLASS,
  LoadEarlierButton,
  StableRow,
  useTranscriptWindow,
  ChannelMessageRow,
  ChatBubble,
  Composer,
  ComposerFrame,
  ComposerSubmit,
  SessionCard,
  Spinner,
  ThreadConversation,
  toast,
  TranscriptDivider,
  UserAvatar,
  UserMessageBubble,
  useLiveArrivals,
  userMessageCaption,
  userMessageTime,
} from "@exp/ui"
import type { User } from "@/db/schema"
import type { SyncedAgent } from "@/components/team/agents-section"
import {
  ComposerInlineTools,
  type ComposerInlineToolsHandle,
} from "@/components/composer-inline-tools"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import { uploadTeamSessionImageFile } from "@/lib/storage/issue-image-upload"
import { sessionCollection } from "@/lib/collections"
import { cn } from "@/lib/utils"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { useNow } from "@/hooks/use-now"
import type { MessagePage } from "@/hooks/use-session-tree"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { messageCollection } from "@/lib/collections"
import {
  buildSessionThread,
  toTreeMessage,
  toTreeSession,
  type MessageRow,
  type SessionRow,
  type ThreadRow,
} from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1293 / NEW-2: a team AGENT's conversation = the CALLER's own
// conversation session with it (a root with `agentId`, `userId` = me) that
// never ends; none yet = an empty conversation until the first message.
// Its messages read as comment rows (avatar or the agent's icon, the name,
// the time, the body); a thread it started (`thread_started`) or a session
// it spawned (`session`) is THE session card under it, opening that thread
// (a root) or that session (a child). Question and plan cards answer in the
// card. Model = `buildSessionThread` over the conversation root with the
// team's sessions as card targets.

/** A card's answer (`sessions.answer`): a question's text + its picked
 *  labels, or a plan's decision + note + EXP-1280 answers by question id;
 *  both carry the inline surface's data model. */
export type CardAnswer = {
  answer?: string
  /** A plan: question id → labels; a question: its labels. */
  answers?: Record<string, string[]> | string[]
  decision?: `approved` | `rejected`
  /** A plan approval that starts in a fresh context. */
  fresh?: boolean
  feedback?: string
  surfaceData?: Record<string, unknown>
}

/** Answer a question / decide a plan IN its card (`sessions.answer`); a
 *  failure toasts `Couldn't answer the question` (a plan: `the plan`).
 *  `pending` = the cards whose answer is in flight. */
export function useAnswerMessage(): {
  answer: (messageId: string, input: CardAnswer) => Promise<void>
  pending: ReadonlySet<string>
} {
  const [pending, setPending] = useState<ReadonlySet<string>>(() => new Set())
  const answer = useCallback(
    async (messageId: string, input: CardAnswer) => {
      setPending((current) => new Set([...current, messageId]))
      try {
        const result = await trpc.sessions.answer.mutate({ messageId, ...input }, SKIP_ERROR_TOAST)
        await messageCollection.utils.awaitTxId(result.txid).catch(() => undefined)
      } catch (error) {
        failedAction(`answer`, input.decision ? `the plan` : `the question`, error)
      } finally {
        setPending((current) => {
          const next = new Set(current)
          next.delete(messageId)
          return next
        })
      }
    },
    []
  )
  return { answer, pending }
}

/** EXP-1317: revoke a person's own queued message (`sessions.unqueue`); a
 *  failure toasts `Couldn't remove the message`. */
export function useUnqueueMessage(): (messageId: string) => Promise<void> {
  return useCallback(async (messageId: string) => {
    try {
      const result = await trpc.sessions.unqueue.mutate({ messageId }, SKIP_ERROR_TOAST)
      await messageCollection.utils.awaitTxId(result.txid).catch(() => undefined)
    } catch (error) {
      failedAction(`remove`, `the message`, error)
    }
  }, [])
}

export function AgentChannel({
  agent,
  conversation,
  messages,
  teamSessions,
  users,
  canAnswer,
  onOpenSession,
  layout = `transcript`,
  onSurfaceReply,
  scrollRef,
  tail,
}: {
  /** Thread 58cb32df (chats.json TRANSCRIPT): `bubbles` = the phones' chat
   *  bubbles (the person right, the agent left in bg-card). */
  layout?: `transcript` | `bubbles`
  /** A surface card's `reply` press (absent = read-only). */
  onSurfaceReply?: (text: string) => void
  /** The `agents` definition (name, icon, color). */
  agent: Pick<SyncedAgent, `name` | `icon` | `color`>
  /** The caller's conversation session with it; null = none yet. */
  conversation: SessionRow | null
  messages: readonly MessageRow[]
  /** Every synced session of the team (the cards' targets). */
  teamSessions: readonly SessionRow[]
  users: readonly User[]
  canAnswer: boolean
  /** A card was tapped: its session row (a root opens its thread). */
  onOpenSession: (session: SessionRow) => void
  /** #EXP-1380: the transcript's scroller (Load earlier keeps its place). */
  scrollRef?: RefObject<HTMLElement | null>
  /** #EXP-1380 (chats.json WORKING): the status line after the last row. */
  tail?: ReactNode
}) {
  // #EXP-1380 (chats.json PERF): the clock only drives the session cards'
  // elapsed time, so it ticks only while a card's session is live.
  const liveCard = useMemo(() => {
    const targets = new Set<string>()
    for (const message of messages) {
      const card = message.card as { kind?: unknown; sessionId?: unknown } | null
      if ((card?.kind === `session` || card?.kind === `thread_started`) && typeof card.sessionId === `string`) {
        targets.add(card.sessionId)
      }
    }
    return teamSessions.some((row) => targets.has(row.id) && row.status !== `ended`)
  }, [messages, teamSessions])
  const now = useNow(liveCard ? 1_000 : 3_600_000)
  // EXP-1334: the thread has no root card any more, so every row is a
  // channel row (the first one is the agent's first message).
  const allRows = useMemo(
    () =>
      conversation
        ? buildSessionThread({
            root: toTreeSession(conversation),
            sessions: teamSessions.map(toTreeSession),
            sessionIssues: [],
            issues: [],
            messages: messages.map(toTreeMessage),
            pullRequests: [],
            now: now.toISOString(),
          }).rows
        : [],
    [conversation, teamSessions, messages, now]
  )
  const ownScrollRef = useRef<HTMLElement | null>(null)
  // #EXP-1380 (chats.json WINDOW): the newest rows, `Load earlier` widens.
  // #EXP-1380: `useTreeMessages` loads the newest messages (its `page`),
  // the window pages more in.
  const page = (messages as { page?: MessagePage }).page
  const transcript = useTranscriptWindow(allRows, scrollRef ?? ownScrollRef, {
    resetKey: conversation?.id ?? null,
    ...page,
  })
  const rows = transcript.rows
  const messagesById = useMemo(() => new Map(messages.map((m) => [m.id, m])), [messages])
  const usersById = useMemo(() => new Map(users.map((user) => [user.id, user])), [users])
  const sessionsById = useMemo(
    () => new Map(teamSessions.map((row) => [row.id, row])),
    [teamSessions]
  )
  const { answer, pending } = useAnswerMessage()
  // The callbacks a skipped (memoized) row holds read the latest ones.
  const latest = useRef({ onOpenSession, onSurfaceReply, sessionsById })
  latest.current = { onOpenSession, onSurfaceReply, sessionsById }

  const agentName = agent.name
  const agentLead = (
    <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className="size-4" />
  )
  const time = (id: string) => {
    const message = messagesById.get(id)
    return message ? userMessageCaption({ at: new Date(message.createdAt) }) : null
  }

  const arrived = useLiveArrivals(rows.map((row) => row.id))
  const bubbles = layout === `bubbles`
  const clock = (id: string) => {
    const message = messagesById.get(id)
    return message ? userMessageTime(new Date(message.createdAt)) : null
  }
  const authorOf = (row: ThreadRow) =>
    row.type === `message` && row.author === `user` && row.authorId
      ? usersById.get(row.authorId)
      : undefined

  const render = (row: ThreadRow) => {
    if (row.type === `divider`) {
      return <TranscriptDivider kind={row.kind} at={row.at} />
    }
    if (row.type === `message` && bubbles && row.author !== `system`) {
      const enter = arrived(row.id) ? CHAT_BUBBLE_ENTER_CLASS : undefined
      return row.author === `user` ? (
        <UserMessageBubble
          variant="chat"
          text={row.body}
          renderText={renderThreadText}
          time={clock(row.id)}
          caption={row.queued ? `queued` : null}
          className={enter}
        />
      ) : (
        <ChatBubble time={clock(row.id)} className={enter}>
          {renderThreadText(row.body)}
        </ChatBubble>
      )
    }
    if (row.type === `message`) {
      const user = authorOf(row)
      const name = row.author === `agent` ? agentName : (user?.name ?? ``)
      return (
        <ChannelMessageRow
          lead={row.author === `agent` ? agentLead : <UserAvatar user={user} size={32} />}
          name={name}
          time={[time(row.id), row.queued ? `queued` : null].filter(Boolean).join(` · `)}
        >
          {renderThreadText(row.body)}
        </ChannelMessageRow>
      )
    }
    if (row.type === `session`) {
      const sessionId = row.card.sessionId
      return (
        <div className={bubbles ? undefined : `pl-11`}>
          <SessionCard
            card={row.card}
            onOpen={
              sessionsById.has(sessionId)
                ? () => {
                    const target = latest.current.sessionsById.get(sessionId)
                    if (target) latest.current.onOpenSession(target)
                  }
                : undefined
            }
          />
        </div>
      )
    }
    return (
      <div className={bubbles ? undefined : `pl-11`}>
        <ThreadConversation
          rows={[row]}
          renderText={renderThreadText}
          attachmentSrc={attachmentSrc}
          onAnswer={canAnswer ? (id, input) => void answer(id, input) : undefined}
          onDecide={canAnswer ? (id, input) => void answer(id, input) : undefined}
          pending={pending}
          onSurfaceReply={onSurfaceReply ? (text) => latest.current.onSurfaceReply?.(text) : undefined}
          arrived={arrived}
        />
      </div>
    )
  }

  return (
    <div data-testid="agent-channel" data-layout={layout} className={cn(`flex flex-col`, bubbles ? `gap-2.5` : `gap-4`)}>
      {transcript.canLoadEarlier ? <LoadEarlierButton onClick={transcript.loadEarlier} /> : null}
      {rows.map((row) => (
        <StableRow
          key={row.id}
          row={row}
          render={render}
          deps={[
            bubbles,
            arrived(row.id),
            row.type === `message` ? clock(row.id) : null,
            row.type === `message` ? time(row.id) : null,
            row.type === `message` ? authorOf(row) : null,
            row.type === `session` ? sessionsById.has(row.card.sessionId) : null,
            pending.has(row.id),
            canAnswer,
            !onSurfaceReply,
            agentName,
            agent.icon,
            agent.color,
          ]}
        />
      ))}
      {tail}
    </div>
  )
}

const attachmentSrc = (id: string) => `/api/attachments/${id}`

interface PendingFile {
  key: string
  file: File
  previewUrl: string | null
}

/** NEW-2: the agent page's composer while the caller has no conversation
 *  with the agent yet: THE composer card (text, mentions, files) whose send
 *  is `sessions.send({agentId, text, attachmentIds})`; the server opens the
 *  conversation session and the page picks it up as it syncs. */
export function AgentComposer({
  teamId,
  agentId,
  appId,
  deviceId,
  disabled = false,
  placeholder,
  users = [],
  testId,
  className,
  onSent,
}: {
  teamId: string
  /** The agent to message (`sessions.send({agentId})`). */
  agentId?: string
  /** VAPP-110: the app whose editor session to open instead
   *  (`sessions.send({appId, deviceId})`). */
  appId?: string
  /** The device a fresh editor session runs on (an app send). */
  deviceId?: string
  disabled?: boolean
  placeholder: string
  users?: readonly User[]
  testId?: string
  className?: string
  /** The conversation session the send opened (or reused). */
  onSent?: (sessionId: string) => void
}) {
  const [text, setText] = useState(``)
  const [files, setFiles] = useState<PendingFile[]>([])
  const [sending, setSending] = useState(false)
  const fieldRef = useRef<MentionTextareaHandle>(null)
  const toolsRef = useRef<ComposerInlineToolsHandle>(null)
  const empty = text.trim().length === 0 && files.length === 0

  const addFiles = (picked: File[]) => {
    if (picked.length === 0) return
    setFiles((current) => [
      ...current,
      ...picked.map((file, index) => ({
        key: `${Date.now()}-${index}-${file.name}`,
        file,
        previewUrl: file.type.startsWith(`image/`) ? URL.createObjectURL(file) : null,
      })),
    ])
  }
  const removeFile = (key: string) =>
    setFiles((current) => {
      const gone = current.find((entry) => entry.key === key)
      if (gone?.previewUrl) URL.revokeObjectURL(gone.previewUrl)
      return current.filter((entry) => entry.key !== key)
    })

  const send = async () => {
    if (sending || empty || disabled) return
    setSending(true)
    try {
      const attachmentIds: string[] = []
      for (const entry of files) {
        const uploaded = await uploadTeamSessionImageFile(teamId, entry.file)
        attachmentIds.push(uploaded.id)
      }
      const result = await trpc.sessions.send.mutate(
        {
          ...(appId ? { appId, deviceId } : { agentId }),
          text: text.trim() || undefined,
          attachmentIds: attachmentIds.length > 0 ? attachmentIds : undefined,
        },
        SKIP_ERROR_TOAST
      )
      for (const entry of files) if (entry.previewUrl) URL.revokeObjectURL(entry.previewUrl)
      setText(``)
      setFiles([])
      await sessionCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      onSent?.(result.sessionId)
    } catch {
      toast.error(`Couldn't send the message`)
    } finally {
      setSending(false)
    }
  }

  return (
    <ComposerFrame data-testid={testId} className={cn(`w-full`, className)}>
      <Composer
        strip={
          files.length > 0 ? (
            <div className="flex flex-wrap gap-2 px-3 pt-3">
              {files.map((entry) =>
                entry.previewUrl ? (
                  <AttachmentThumb
                    key={entry.key}
                    src={entry.previewUrl}
                    alt={entry.file.name}
                    onRemove={() => removeFile(entry.key)}
                  />
                ) : (
                  <AttachmentFileTile
                    key={entry.key}
                    name={entry.file.name}
                    onRemove={() => removeFile(entry.key)}
                  />
                )
              )}
            </div>
          ) : undefined
        }
        tools={
          <ComposerInlineTools
            ref={toolsRef}
            fieldRef={fieldRef}
            onFiles={addFiles}
            disabled={sending || disabled}
          />
        }
        submit={
          <ComposerSubmit
            aria-label="Send"
            disabled={empty || sending || disabled}
            onClick={() => void send()}
          >
            {sending ? <Spinner size={16} label="Sending" /> : undefined}
          </ComposerSubmit>
        }
        onDrop={(event) => {
          if (event.dataTransfer.files.length === 0) return
          event.preventDefault()
          addFiles(Array.from(event.dataTransfer.files))
        }}
        onDragOver={(event) => {
          if (event.dataTransfer.types.includes(`Files`)) event.preventDefault()
        }}
      >
        <MentionTextarea
          ref={fieldRef}
          value={text}
          onValueChange={setText}
          users={users as User[]}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing) return
            if (event.key !== `Enter` || event.shiftKey) return
            event.preventDefault()
            void send()
          }}
          onPaste={(event) => {
            if (event.clipboardData.files.length === 0) return
            event.preventDefault()
            addFiles(Array.from(event.clipboardData.files))
          }}
          placeholder={placeholder}
          className="min-h-20 resize-none border-0 bg-transparent px-3 pt-3 shadow-none focus-visible:ring-0"
          maxLength={SESSIONS_SEND_TEXT_MAX}
        />
      </Composer>
    </ComposerFrame>
  )
}
