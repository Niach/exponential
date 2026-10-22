import { useCallback, useMemo, useRef, useState } from "react"
import { SESSIONS_SEND_TEXT_MAX } from "@exp/db-schema/domain"
import {
  AttachmentFileTile,
  AttachmentThumb,
  BoardGlyph,
  ChannelMessageRow,
  Composer,
  ComposerFrame,
  ComposerSubmit,
  SessionCard,
  Spinner,
  ThreadConversation,
  toast,
  UserAvatar,
  userMessageCaption,
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
}: {
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
}) {
  const now = useNow(1_000)
  // EXP-1334: the thread has no root card any more, so every row is a
  // channel row (the first one is the agent's first message).
  const rows = useMemo(
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
  const messagesById = useMemo(() => new Map(messages.map((m) => [m.id, m])), [messages])
  const usersById = useMemo(() => new Map(users.map((user) => [user.id, user])), [users])
  const sessionsById = useMemo(
    () => new Map(teamSessions.map((row) => [row.id, row])),
    [teamSessions]
  )
  const { answer, pending } = useAnswerMessage()

  const agentName = agent.name
  const agentLead = (
    <BoardGlyph board={{ icon: agent.icon, color: agent.color }} className="size-4" />
  )
  const time = (id: string) => {
    const message = messagesById.get(id)
    return message ? userMessageCaption({ at: new Date(message.createdAt) }) : null
  }

  const render = (row: ThreadRow) => {
    if (row.type === `message`) {
      const user = row.author === `user` && row.authorId ? usersById.get(row.authorId) : undefined
      const name = row.author === `agent` ? agentName : (user?.name ?? ``)
      return (
        <ChannelMessageRow
          key={row.id}
          lead={row.author === `agent` ? agentLead : <UserAvatar user={user} size={32} />}
          name={name}
          time={[time(row.id), row.queued ? `queued` : null].filter(Boolean).join(` · `)}
        >
          {renderThreadText(row.body)}
        </ChannelMessageRow>
      )
    }
    if (row.type === `session`) {
      const target = sessionsById.get(row.card.sessionId)
      return (
        <div key={row.id} className="pl-11">
          <SessionCard
            card={row.card}
            onOpen={target ? () => onOpenSession(target) : undefined}
          />
        </div>
      )
    }
    return (
      <div key={row.id} className="pl-11">
        <ThreadConversation
          rows={[row]}
          renderText={renderThreadText}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          onAnswer={canAnswer ? (id, input) => void answer(id, input) : undefined}
          onDecide={canAnswer ? (id, input) => void answer(id, input) : undefined}
          pending={pending}
        />
      </div>
    )
  }

  return (
    <div data-testid="agent-channel" className="flex flex-col gap-4">
      {rows.map(render)}
    </div>
  )
}

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
  placeholder,
  users = [],
  testId,
  className,
  onSent,
}: {
  teamId: string
  agentId: string
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
    if (sending || empty) return
    setSending(true)
    try {
      const attachmentIds: string[] = []
      for (const entry of files) {
        const uploaded = await uploadTeamSessionImageFile(teamId, entry.file)
        attachmentIds.push(uploaded.id)
      }
      const result = await trpc.sessions.send.mutate(
        {
          agentId,
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
            disabled={sending}
          />
        }
        submit={
          <ComposerSubmit
            aria-label="Send"
            disabled={empty || sending}
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
