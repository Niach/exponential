import { createFileRoute } from "@tanstack/react-router"
import { VOICE_COPY } from "@exp/domain-contract/voice"
import { trpc } from "@/lib/trpc-client"
import { VoiceSettingsSection } from "@/components/account/voice-settings-card"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/voice`)({
  head: () => ({
    meta: [{ title: pageTitle(VOICE_COPY.group, `Settings`) }],
  }),
  loader: async () => {
    const settings = await trpc.voice.get.query()
    return { settings }
  },
  component: SettingsVoice,
})

// NEW-67 (voice.json): Personal → Voice, the member's one voice settings row.
function SettingsVoice() {
  const { settings } = Route.useLoaderData()

  return <VoiceSettingsSection settings={settings} />
}
