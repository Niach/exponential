import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"

interface ImagePreviewDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  src: string
  alt?: string
  // Accessible dialog title (sr-only) — usually the filename or alt text.
  label: string
}

/**
 * The shared image lightbox (EXP-316): a borderless dialog hugging the image.
 * Used by the description editor's image node view and the storage table's
 * filename preview.
 */
export function ImagePreviewDialog({
  open,
  onOpenChange,
  src,
  alt,
  label,
}: ImagePreviewDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        // The one dialog that stays a full-screen PAGE below `sm` (EXP-687):
        // a lightbox wants the whole viewport, not a sheet.
        mobile="page"
        // sm:w-auto opts out of DialogContent's sm:w-[calc(100%-2rem)]
        // gutter width — the lightbox panel hugs the image instead of
        // spanning the viewport with the image left-aligned inside it.
        // The base panel is a flex column, so below `sm` — where it is the
        // full-screen page — the image is centred with justify/items-center
        // (the grid-era content-center/justify-items-center equivalents).
        className="w-auto max-w-[min(96vw,80rem)] p-2 max-sm:w-auto max-sm:items-center max-sm:justify-center max-sm:p-2 sm:w-auto sm:max-w-[min(96vw,80rem)]"
        aria-describedby={undefined}
      >
        <DialogTitle className="sr-only">{label}</DialogTitle>
        <img
          src={src}
          alt={alt}
          className="max-h-[85vh] w-auto max-w-full rounded-md object-contain"
        />
      </DialogContent>
    </Dialog>
  )
}
