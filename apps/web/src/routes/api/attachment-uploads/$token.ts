import { createFileRoute } from "@tanstack/react-router"
import { handleSignedAttachmentUpload } from "@/lib/storage/signed-attachment-upload-route"

// EXP-929: the signed issue-attachment upload (the handler and why it is
// anonymous: lib/storage/signed-attachment-upload-route.ts).
export const Route = createFileRoute(`/api/attachment-uploads/$token`)({
  server: {
    handlers: {
      PUT: handleSignedAttachmentUpload,
      POST: handleSignedAttachmentUpload,
    },
  },
})
