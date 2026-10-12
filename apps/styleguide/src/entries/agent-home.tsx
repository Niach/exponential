import composerMenu from "@exp/domain-contract/fixtures/composer-menu.json"
import {
  HOME_GEOMETRY,
  HOME_PLACEHOLDER,
  homeGreeting,
} from "@exp/domain-contract/home-greeting"
import {
  Composer,
  ComposerSubmit,
  ComposerTool,
  ExponentialLogo,
  conceptIcon,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1253 (EXP-1259 D1): the Agent page is HOME (composer-home.json, draft
// 1 + 2). The page itself lives in apps/web (`routes/t/$teamSlug/agent.tsx`)
// and cannot be imported here, so the specimen composes the same `@exp/ui`
// parts the route does: the disc mark + the greeting (the pure
// `homeGreeting`, at a pinned evening), the composer card asking
// "What do you want to build?" and a muted options line. Nothing beneath it.

const UiAddIcon = conceptIcon(`ui-add`)

function Headline({ phone }: { phone?: boolean }) {
  const mark = phone ? HOME_GEOMETRY.markPhone : HOME_GEOMETRY.mark
  return (
    <div className="flex items-center justify-center" style={{ gap: HOME_GEOMETRY.gap }}>
      <span className="flex shrink-0 text-foreground" style={{ width: mark, height: mark }}>
        <ExponentialLogo variant="light" size={mark} className="size-full" />
      </span>
      <h1
        className="leading-tight tracking-tight text-foreground"
        style={{
          fontSize: phone ? HOME_GEOMETRY.headlinePhone : HOME_GEOMETRY.headline,
          fontWeight: HOME_GEOMETRY.headlineWeight,
        }}
      >
        {homeGreeting(`2026-10-12T19:30`, `Danny Straehhuber`)}
      </h1>
    </div>
  )
}

function Home({ phone }: { phone?: boolean }) {
  return (
    <div className={`flex flex-col ${phone ? `w-[24rem]` : `w-[40rem]`} max-w-full`}>
      <Headline phone={phone} />
      <div style={{ marginTop: HOME_GEOMETRY.headlineToCard }} className="grid gap-2">
        <Composer
          tools={
            <ComposerTool aria-label={composerMenu.plusLabel} title={composerMenu.plusLabel}>
              <UiAddIcon />
            </ComposerTool>
          }
          submit={<ComposerSubmit aria-label="Start chat" title="Start chat" />}
        >
          <div className="min-h-20 px-3 pt-3 text-sm text-muted-foreground">{HOME_PLACEHOLDER}</div>
        </Composer>
        <div className="flex flex-wrap items-center gap-x-3 px-1 text-xs text-muted-foreground">
          <span className="text-foreground">macbook</span>
          <span>Exponential repo</span>
          <span className="ml-auto">Opus 5.5</span>
          <span>High</span>
          <span>Plan off</span>
        </div>
      </div>
    </div>
  )
}

export const entry: StyleguideEntry = {
  id: `agent-home`,
  section: `special`,
  owner: `EXP-1253`,
  title: `Agent home`,
  blurb: `Home on every client (fixture \`composer-home.json\`). The disc mark + the time-of-day greeting (${HOME_GEOMETRY.headline}px, phones ${HOME_GEOMETRY.headlinePhone}px), then the composer ("${HOME_PLACEHOLDER}") and its options line. Nothing beneath it; a team with no board gets Getting started ${HOME_GEOMETRY.composerToGettingStarted}px below.`,
  status: {
    web: {
      state: `ok`,
      symbol: `AgentPage / HomeColumn`,
      file: `apps/web/src/routes/t/$teamSlug/agent.tsx`,
      note: `greeting = @exp/domain-contract home-greeting.ts`,
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
