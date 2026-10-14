import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"
import {
  HOME_GEOMETRY,
  HOME_PLACEHOLDER,
} from "@exp/domain-contract/home-greeting"
import {
  Composer,
  ComposerSubmit,
  ComposerTool,
  ExponentialLogo,
  conceptIcon,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1253 / EXP-1293: the Agent page is HOME (composer-home.json). The page
// itself lives in apps/web (`routes/t/$teamSlug/agent.tsx`) and cannot be
// imported here, so the specimen composes the same `@exp/ui` parts the route
// does: the DAWN (FEED-100: the big mark's top half rising over the card,
// 24px off it), the composer card asking "What do you want to build?" and a
// muted options line. No greeting, nothing beneath it.

const UiAddIcon = conceptIcon(`ui-add`)

function Dawn({ phone }: { phone?: boolean }) {
  const size = phone ? 160 : 520
  return (
    <div
      aria-hidden
      className="pointer-events-none mx-auto mb-6 shrink-0 overflow-hidden text-foreground opacity-[0.05] select-none"
      style={{ width: size, height: size / 2 }}
    >
      <ExponentialLogo variant="light" size={size} />
    </div>
  )
}

function Home({ phone }: { phone?: boolean }) {
  return (
    <div className={`flex flex-col ${phone ? `w-[24rem]` : `w-[40rem]`} max-w-full`}>
      <Dawn phone={phone} />
      <div className="grid gap-2">
        <Composer
          tools={
            <ComposerTool aria-label={composerMenu.plusLabel} title={composerMenu.plusLabel}>
              <UiAddIcon />
            </ComposerTool>
          }
          submit={<ComposerSubmit aria-label="Start session" title="Start session" />}
        >
          <div className="min-h-20 px-3 pt-3 text-sm text-muted-foreground">{HOME_PLACEHOLDER}</div>
        </Composer>
        <div className="flex flex-wrap items-center gap-x-3 px-1 text-xs text-muted-foreground">
          <span className="text-foreground">Alex's MacBook Pro</span>
          <span>demo@exponential.at</span>
          <span>Fable</span>
          <span>Plan off</span>
          <span>acme/mobile-app</span>
        </div>
      </div>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `agent-home`,
  section: `special`,
  owner: `EXP-1293`,
  title: `Agent home`,
  blurb: `Home on every client (fixture \`composer-home.json\`). The dawn (FEED-100: the big brand mark's top half, faint, 24px over the card; phones 160px), then the composer ("${HOME_PLACEHOLDER}") and its options line. No greeting, no history button, nothing beneath it; a start opens its thread; a team with no board gets Getting started ${HOME_GEOMETRY.composerToGettingStarted}px below; phones keep a Running list of live threads under it.`,
  status: {
    web: {
      state: `ok`,
      symbol: `AgentPage / HomeColumn`,
      file: `apps/web/src/routes/t/$teamSlug/agent.tsx`,
      note: `HomeDawn + LaunchComposer (sessions.send)`,
    },
    desktop: {
      state: `ok`,
      symbol: `home_greeting::home_greeting`,
      file: `apps/desktop/crates/ui/src/home_greeting.rs`,
      note: `the column = the chat screen's home state`,
    },
    ios: {
      state: `ok`,
      symbol: `AgentPageView`,
      file: `apps/ios/Exponential/UI/Agent/AgentPageView.swift`,
      note: `greeting = ExpCore HomeGreeting.swift`,
    },
    android: {
      state: `ok`,
      symbol: `AgentScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/agent/AgentScreen.kt`,
      note: `greeting = domain/HomeGreeting.kt`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-10">
      <Home />
      <Home phone />
    </div>
  ),
}
