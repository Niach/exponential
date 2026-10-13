import { useRef, useState } from "react"
import { SESSIONS_SEND_TEXT_MAX } from "@exp/db-schema/domain"
import {
  AttachmentFileTile,
  AttachmentThumb,
  Composer,
  ComposerSubmit,
  ComposerTool,
  Spinner,
  conceptIcon,
  toast,
} from "@exp/ui"
import type { User } from "@/db/schema"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import { uploadTeamSessionImageFile } from "@/lib/storage/issue-image-upload"
import { messageCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { cn } from "@/lib/utils"

const AddIcon = conceptIcon(`ui-add`)

// EXP-1293 Threads v2: THE reply box ×4 = the home's composer box (the
// Composer primitive), never a new box. It posts ONE message to a session
// (`sessions.send({sessionId})`): a thread root (an ended thread continues
// server-side in a new child), an agent's channel, or an opened session.
// Stop = `sessions.stop` while the session works.

interface PendingFile {
  key: string
  file: File
  previewUrl: string | null
}

export function SessionComposer({
  teamId,
  sessionId,
  placeholder,
  users = [],
  busy = false,
  canStop = false,
  testId,
  className,
}: {
  teamId: string
  sessionId: string
  placeholder: string
  users?: readonly User[]
  /** The session works right now: the submit offers Stop when `canStop`. */
  busy?: boolean
  canStop?: boolean
  testId?: string
  className?: string
}) {
  const [text, setText] = useState(``)
  const [files, setFiles] = useState<PendingFile[]>([])
  const [sending, setSending] = useState(false)
  const fieldRef = useRef<MentionTextareaHandle>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

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

  const empty = text.trim().length === 0 && files.length === 0

  const send = async () => {
    if (sending || empty) return
    setSending(true)
    try {
      const attachmentIds: string[] = []
      for (const entry of files) {
        const uploaded = await uploadTeamSessionImageFile(teamId, entry.file)
        attachmentIds.push(uploaded.id)
      }
      const result = await trpc.sessions.send.mutate({
        teamId,
        sessionId,
        text: text.trim() || undefined,
        attachmentIds: attachmentIds.length > 0 ? attachmentIds : undefined,
      })
      for (const entry of files) if (entry.previewUrl) URL.revokeObjectURL(entry.previewUrl)
      setText(``)
      setFiles([])
      // A message to a live session writes only a `messages` row: free the
      // composer now, then wait (bounded) for that row to sync.
      setSending(false)
      await messageCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    } catch {
      toast.error(`Couldn't send the message`)
    } finally {
      setSending(false)
    }
  }

  const stop = async () => {
    try {
      await trpc.sessions.stop.mutate({ sessionId })
    } catch {
      toast.error(`Couldn't stop the session`)
    }
  }

  const showStop = canStop && busy && empty && !sending

  return (
    <Composer
      data-testid={testId}
      className={cn(`w-full`, className)}
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
        <>
          <input
            ref={fileInputRef}
            type="file"
            multiple
            hidden
            onChange={(event) => {
              addFiles(Array.from(event.target.files ?? []))
              event.target.value = ``
            }}
          />
          <ComposerTool aria-label="Add file" onClick={() => fileInputRef.current?.click()}>
            <AddIcon className="size-4" />
          </ComposerTool>
        </>
      }
      submit={
        <ComposerSubmit
          stop={showStop}
          aria-label={showStop ? `Stop` : `Send`}
          disabled={!showStop && (empty || sending)}
          onClick={showStop ? stop : send}
        >
          {sending ? <Spinner size={16} label="Sending" /> : undefined}
        </ComposerSubmit>
      }
    >
      <MentionTextarea
        ref={fieldRef}
        value={text}
        onValueChange={setText}
        users={users as User[]}
        onKeyDown={(event) => {
          if (event.key !== `Enter` || event.shiftKey) return
          if (event.nativeEvent.isComposing) return
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
  )
}
