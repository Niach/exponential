import { createFileRoute } from "@tanstack/react-router"
import { errorToResponse } from "@/lib/http-errors"
import { handleWebhookDelivery } from "@/lib/apps/webhooks"

// #VAPP-237 (eApps A3): an app's inbound webhook (lib/apps/webhooks.ts):
// a signed delivery runs the published package's automations on the hook.
export const Route = createFileRoute(`/api/apps/hooks/$appId/$name`)({
  server: {
    handlers: {
      POST: async ({ request, params }) => {
        try {
          return await handleWebhookDelivery(request, params.appId, params.name)
        } catch (error) {
          return errorToResponse(error, request)
        }
      },
    },
  },
})
