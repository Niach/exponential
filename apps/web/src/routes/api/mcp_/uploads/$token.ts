import { createFileRoute } from "@tanstack/react-router"
import { handleSignedAttachmentUpload } from "@/lib/storage/signed-attachment-upload-route"

// #NEW-149 (packages/emcp fixtures/files.json `staging`): the MCP files
// extension's upload target, `${APP_URL}/api/mcp/uploads/<token>`. The SAME
// signed path as /api/attachment-uploads/$token: the token (minted by the
// `upload_required` answer after the tool's access checks) is the whole
// authorization, binds the attachment, the uploader, the type, a 15-minute
// window and the file's declared size as the cap. No cookie or header is
// read (`credentials: omit`); the tool's retry finalizes.
export const Route = createFileRoute(`/api/mcp_/uploads/$token`)({
  server: {
    handlers: {
      PUT: handleSignedAttachmentUpload,
      POST: handleSignedAttachmentUpload,
    },
  },
})
