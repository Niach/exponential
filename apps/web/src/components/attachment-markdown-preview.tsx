import { useEffect, useState } from "react"
import { Download, LoaderCircle } from "lucide-react"
import {
  Button,
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@exp/ui"
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

  useEffect(() => {
    if (!open || !attachment) return
    let cancelled = false
    setState({ status: `loading` })
    loadMarkdownPreview(attachment)
      .then((next) => {
        if (!cancelled) setState(next)
      })
      .catch((error: unknown) => {
        if (cancelled) return
        setState({
          status: `error`,
          message:
            error instanceof Error ? error.message : `Couldn't load this file.`,
        })
      })
    return () => {
      cancelled = true
    }
  }, [open, attachment])

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
            <p className="flex items-center gap-1.5 py-6 text-xs text-muted-foreground">
              <LoaderCircle className="size-3 animate-spin" />
              Loading...
            </p>
          )}
          {state.status === `too-large` && (
            <p className="py-6 text-sm text-muted-foreground">
              This file is too large to preview here. Download it to read it.
            </p>
          )}
          {state.status === `error` && (
            <p className="py-6 text-sm text-destructive">{state.message}</p>
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
