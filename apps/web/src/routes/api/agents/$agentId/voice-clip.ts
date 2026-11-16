import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleAgentVoiceClipDelete, handleAgentVoiceClipUpload } from "@/lib/storage/agent-voice-clip-upload"

// #EXP-1847 (agent-settings.json `voice.clipUpload`): an agent's voice clip.
// Its editors only; POST one multipart part "file" (a WAV) → {clipId,
// clipName}; DELETE clears it.
export const Route = createFileRoute(`/api/agents/$agentId/voice-clip`)({
  server: {
    handlers: {
      POST: async (context) => {
        try {
          return await handleAgentVoiceClipUpload(context)
        } catch (error) {
          return errorToResponse(error, context.request)
        }
      },
      DELETE: async (context) => {
        try {
          return await handleAgentVoiceClipDelete(context)
        } catch (error) {
          return errorToResponse(error, context.request)
        }
      },
    },
  },
})
