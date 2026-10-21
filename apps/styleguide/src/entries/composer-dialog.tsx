import { contract } from "@exp/domain-contract"
import { MenuPanel, Pill, conceptIcon } from "@exp/ui"

import { ComposerSpecimen, composerMenuEntries } from "./composer-specimen.tsx"
import type { StyleguideEntry } from "./types.ts"

// EXP-1019 — the START-CODING DIALOG; EXP-1249 — its ONE "+" menu; EXP-1334
// — the ONE composer (the start, the thread and the session view alike).
//
// Drawn from the REAL parts (`composer-specimen.tsx`): `ComposerFrame` /
// `Composer` / `ComposerEditTools` / `ComposerOptionsRow` are the halves the
// app's `LaunchComposer` and `SessionComposer` compose, and the open "+" is
// `MenuPanel` (the app's `Menu` renderer at rest) fed the rows of
// `composer-menu.json`, the fixture the four clients replay. The words are
// the contract's (`contract.composerUi`, ×4) so the demo cannot drift from
// what the clients print. The app components live in apps/web and cannot be
// imported here.

const { composerUi } = contract

const PromptIcon = conceptIcon(`nav-actions`)

function Caption({ children }: { children: string }) {
  return <p className="text-xs text-muted-foreground">{children}</p>
}

export const entry: StyleguideEntry = {
  id: `composer-dialog`,
  section: `special`,
  owner: `EXP-1019`,
  title: `Composer dialog`,
  blurb: `THE launcher, opened over wherever the play button was pressed instead of navigating away from it, and (EXP-1334) THE composer of a thread and a session view: one card everywhere. The SUBJECT leads a start: Implement + the removable issue chips, or (NEW-2) a prompt picked off the / menu, its chip alone; the field under it is the SECONDARY half ("Additional instructions (optional)…"). In the card: the "+" = the options menu (EXP-1249: Implement issue › and Message an agent › before a start only, Add file or image, Effort ›, Ultracode, MCP servers ›, Computer use; in a session the same rows change the live session through sessions.configure), then the inline edit tools (Add image, Attach files, #, emoji), Send ⇄ Stop on the right. Under the card the muted options line (see Composer options). A start = ONE sessions.send; the dialog shuts and the new thread opens.`,
  status: {
    web: {
      state: `ok`,
      symbol: `LaunchDialogHost / LaunchComposer / SessionComposer / ComposerOptionsMenu`,
      file: `apps/web/src/components/launch-dialog/launch-dialog.tsx`,
      note: `the start card is launch-composer.tsx, the thread/session card session-composer.tsx; both = @exp/ui ComposerFrame + composer-inline-tools.tsx, the "+" = launch-dialog/composer-plus-menu.tsx (rows: composer-menu.ts over fixtures/composer-menu.json)`,
    },
    desktop: {
      state: `ok`,
      symbol: `composer_dialog::open`,
      file: `apps/desktop/crates/ui/src/composer_dialog.rs`,
      note: `one launcher in two presentations (chat_screen.rs); the "+" = chat_screen::plus_menu over composer-menu.json`,
    },
    ios: {
      state: `ok`,
      symbol: `AgentComposerHeadline / ComposerPlusMenu`,
      file: `apps/ios/Exponential/UI/Agent/AgentComposerCard.swift`,
      note: `a pushed page on the phone; the "+" sheet = ComposerPlusMenu.swift over composer-menu.json`,
    },
    android: {
      state: `ok`,
      symbol: `AgentComposerHeadline / ComposerPlusMenuSheet`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/agent/AgentComposer.kt`,
      note: `a screen on the phone; the "+" sheet = ComposerPlusMenu.kt over composer-menu.json`,
    },
  },
  island: () => (
    <div className="grid gap-10">
      <div className="flex flex-wrap items-start gap-8">
        <div className="grid w-[36rem] max-w-full gap-2">
          <Caption>A prompt off the / menu: its chip IS the headline, and the send is the only thing left to do.</Caption>
          <div className="flex flex-wrap items-center gap-2 px-1">
            <Pill size="sm" mode="readonly">
              <PromptIcon aria-hidden />
              Triage new issues
            </Pill>
          </div>
          <ComposerSpecimen phase="start" placeholder={composerUi.instructionsPlaceholder} submitLabel="Start session" />
        </div>
        <div className="grid gap-2">
          <MenuPanel entries={composerMenuEntries(`start`)} density="pointer" />
          <Caption>the "+" before a start: composer-menu.json, every condition met</Caption>
        </div>
      </div>
      <div className="flex flex-wrap items-start gap-8">
        <div className="grid w-[36rem] max-w-full gap-2">
          <Caption>A running session: Stop, model · Plan … the context ring under the card.</Caption>
          <ComposerSpecimen phase="running" placeholder="Type / for commands" />
          <Caption>An idle thread: the same composer (it messages the dispatcher).</Caption>
          <ComposerSpecimen phase="idle" placeholder="Type / for commands" />
        </div>
        <div className="grid gap-2">
          <MenuPanel entries={composerMenuEntries(`running`)} density="pointer" />
          <Caption>the "+" in a session: no subject rows, the options change the live session</Caption>
        </div>
      </div>
      <div className="grid w-[36rem] max-w-full gap-2">
        <Caption>A chat in the dialog: no subject, the contract's placeholder.</Caption>
        <ComposerSpecimen phase="start" placeholder={composerUi.chatPlaceholder} />
      </div>
    </div>
  ),
}
