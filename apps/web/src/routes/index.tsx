import { createFileRoute, redirect } from "@tanstack/react-router"

// No active team anywhere: md+ lands on Home (#EXP-2027), a phone on its
// Chats tab (#EXP-2109; `useIsMobile`'s 768px breakpoint). The team-less
// layout (`_app`) does the sign-in and onboarding redirects.
export const Route = createFileRoute(`/`)({
  ssr: false,
  beforeLoad: () => {
    const phone = typeof window !== `undefined` && window.innerWidth < 768
    throw redirect({ to: phone ? `/chats` : `/home` })
  },
})
