import { useEffect, useState } from "react"
import { Download } from "lucide-react"
import {
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  SkeletonRows,
} from "@exp/ui"
import { failedAction } from "@/lib/action-error"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import {
  MARKDOWN_PREVIEW_MAX_BYTES,
  buildAttachmentDownloadUrl,
  formatAttachmentSize,
} from "@/lib/attachment-files"

/** The row fields the preview needs — every attachment list has them. */
export interface MarkdownPreviewTarget {
  id: string
  url: string
  filename: string
  sizeBytes: number
}

interface AttachmentMarkdownPreviewDialogProps {
  attachment: MarkdownPreviewTarget | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

type PreviewState =
  | { status: `loading` }
  | { status: `ready`; markdown: string }
  | { status: `too-large` }
  | { status: `error`; message: string }

/**
 * Fetches the attachment's text through the byte route (same-origin cookie
 * auth, like every `<img>` embed). Exported for the dialog's test; the
 * size ceiling is checked twice — from the synced row before the fetch, and
 * from the body after it for rows that never recorded a size.
 */
export async function loadMarkdownPreview(
  attachment: MarkdownPreviewTarget,
  fetchImpl: typeof fetch = fetch
): Promise<PreviewState> {
  if (attachment.sizeBytes > MARKDOWN_PREVIEW_MAX_BYTES) {
    return { status: `too-large` }
  }
  const response = await fetchImpl(attachment.url, {
    credentials: `same-origin`,
  })
  if (!response.ok) {
    return {
      status: `error`,
      message:
        response.status === 404
          ? `This file is no longer available.`
          : `Couldn't load this file (HTTP ${response.status}).`,
    }
  }
  const text = await response.text()
  if (text.length > MARKDOWN_PREVIEW_MAX_BYTES) {
    return { status: `too-large` }
  }
  return { status: `ready`, markdown: text }
}

/**
 * EXP-955: the in-app preview for `.md` attachments — the same read-only
 * TipTap renderer that draws descriptions, comments and What's new, so a
 * research note attached to an issue reads like the issue itself (issue
 * refs chip, images embed, tables lay out) instead of downloading. Download
 * stays one click away in the footer.
 */
export function AttachmentMarkdownPreviewDialog({
  attachment,
  open,
  onOpenChange,
}: AttachmentMarkdownPreviewDialogProps) {
  const [state, setState] = useState<PreviewState>({ status: `loading` })
  // Bumped by the failure toast's Retry: the effect fetches again.
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    if (!open || !attachment) return
    let cancelled = false
    setState({ status: `loading` })
    // FEED-79: a failed load = the empty body + ONE toast (Retry refetches).
    const fail = (message: string) => {
      setState({ status: `error`, message })
      failedAction(`load`, `the preview`, message, {
        retry: () => setAttempt((count) => count + 1),
      })
    }
    loadMarkdownPreview(attachment)
      .then((next) => {
        if (cancelled) return
        if (next.status === `error`) fail(next.message)
        else setState(next)
      })
      .catch((error: unknown) => {
        if (cancelled) return
        fail(error instanceof Error ? error.message : `Couldn't load this file.`)
      })
    return () => {
      cancelled = true
    }
  }, [open, attachment, attempt])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        mobile="sheet-full"
        className="sm:max-h-[85dvh] sm:max-w-3xl"
        data-testid="attachment-markdown-preview"
      >
        <DialogHeader>
          <DialogTitle className="truncate">
            {attachment?.filename ?? `Preview`}
          </DialogTitle>
          <DialogDescription>
            {attachment && attachment.sizeBytes > 0
              ? `Markdown · ${formatAttachmentSize(attachment.sizeBytes)}`
              : `Markdown`}
          </DialogDescription>
        </DialogHeader>
        <DialogBody className="text-sm text-foreground">
          {state.status === `loading` && (
            <SkeletonRows dot={false} rowHeight={28} className="-mx-3 py-4" />
          )}
          {state.status === `too-large` && (
            <p className="py-6 text-sm text-muted-foreground">
              This file is too large to preview here. Download it to read it.
            </p>
          )}
          {state.status === `error` && (
            <p className="py-6 text-sm text-muted-foreground">
              No preview. Download it to read it.
            </p>
          )}
          {state.status === `ready` && (
            <MarkdownEditor
              editable={false}
              markdown={state.markdown}
              onChange={() => {}}
              ariaLabel={attachment?.filename}
            />
          )}
        </DialogBody>
        <DialogFooter>
          {attachment && (
            <Button variant="outline" asChild>
              <a
                href={buildAttachmentDownloadUrl(attachment.url)}
                download={attachment.filename}
              >
                <Download />
                Download
              </a>
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
