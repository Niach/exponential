import { contract } from "@exp/domain-contract"

import {
  escapeHtml,
  svgCircleArrowUp,
  svgGitMerge,
  svgHash,
  svgPlay,
  svgPlus,
  svgX,
} from "../html.ts"
import type { StyleguideEntry } from "./types.ts"

// EXP-1019 — the START-CODING DIALOG, filled from the EXP-1029 placeholder.
//
// Hand-written rather than an island: the arrangement is what is special
// here, and it belongs to no single component. The frame is `dialog`, the
// card is `composer`, the chips are `pill`/`issue-chip` — all three are
// documented on their own, and an island of any of them would show the part
// instead of the ORDER.
//
// The words are the contract's (`contract.composerUi`, ×4) so the demo cannot
// drift from what the four clients actually print.

const { composerUi } = contract

/** An issue chip leads with its status glyph; the `pill` dot stands in here. */
const ISSUE_DOT = `<span class="dot"></span>`

/**
 * A removable subject chip: the `pill` capsule, readonly, with a trailing ✕.
 * `leading` is the action's own icon, or the status dot an issue chip carries.
 */
function chip(label: string, leading: string): string {
  return [
    `<span class="cmp-pill" data-size="sm" data-mode="readonly">`,
    leading,
    `<span class="label">${escapeHtml(label)}</span>`,
    svgX,
    `</span>`,
  ].join(``)
}

/** The headline: the contract's verb, then whatever the run is about. */
function headline(verb: string, chips: string): string {
  return `<div class="header"><span class="title">${escapeHtml(verb)}</span>${chips}</div>`
}

/** The card: the secondary field, the three tools and the round send. */
function card(placeholder: string): string {
  return [
    `<div class="card">`,
    `<div class="field">${escapeHtml(placeholder)}</div>`,
    `<div class="tool-row">`,
    `<button class="cmp-ghost-icon-button" type="button" title="Issues">${svgHash}</button>`,
    `<button class="cmp-ghost-icon-button" type="button" title="Actions">${svgPlay}</button>`,
    `<button class="cmp-ghost-icon-button" type="button" title="Attach image">${svgPlus}</button>`,
    `<button class="cmp-icon-button" type="button" title="Start">${svgCircleArrowUp}</button>`,
    `</div>`,
    `</div>`,
  ].join(``)
}

/**
 * The muted options line under the card: the machine, the login (which
 * implies the agent), the model, the Plan switch, and `⋯` for the rest.
 */
function options(account: string, model: string): string {
  return [
    `<div class="footer">`,
    `<span>Device <span class="value">buildbox</span></span>`,
    `<span>Account <span class="value">${escapeHtml(account)}</span></span>`,
    `<span>Model <span class="value">${escapeHtml(model)}</span></span>`,
    `<span>Plan</span><span class="cmp-switch"></span>`,
    `<span class="value">⋯</span>`,
    `</div>`,
  ].join(``)
}

function launcher(parts: { caption: string; verb: string; chips: string; account: string; model: string }): string {
  return [
    `<div class="cmp-launch">`,
    `<p class="caption">${escapeHtml(parts.caption)}</p>`,
    headline(parts.verb, parts.chips),
    card(composerUi.instructionsPlaceholder),
    options(parts.account, parts.model),
    `</div>`,
  ].join(``)
}

export const entry: StyleguideEntry = {
  id: `composer-dialog`,
  section: `special`,
  owner: `EXP-1019`,
  title: `Composer dialog`,
  blurb: `THE launcher, opened over wherever the play button was pressed instead of navigating away from it. The SUBJECT leads: the contract's verb (Run for an action, Implement for issues, ×4) followed by the removable subject chips, set as the biggest thing in the dialog — a prefilled action used to be a small badge inside a prompt box, so nobody realised the thing they picked was already loaded and one press away. The field under it is the SECONDARY half and says so ("Additional instructions (optional)…", or an action's own promptPlaceholder); under that the three tools (issues, actions, image) and the round send, then the muted options line: device, account, model, the Plan switch and ⋯ for the rest. A subject-LESS chat still opens the Agent page, where a chat belongs, and the same composer draws both — there is no second launcher to drift.`,
  status: {
    web: {
      state: `ok`,
      symbol: `LaunchDialogHost / LaunchComposer / LaunchHeadline`,
      file: `apps/web/src/components/launch-dialog/launch-dialog.tsx`,
      note: `the card is launch-composer.tsx, the headline launch-dialog/launch-headline.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `composer_dialog::open`,
      file: `apps/desktop/crates/ui/src/composer_dialog.rs`,
      note: `the composer is ChatScreenView in its Dialog presentation (chat_screen.rs): one launcher, two presentations`,
    },
    ios: {
      state: `leftover`,
      symbol: `AgentComposerHeadline`,
      file: `apps/ios/Exponential/UI/Agent/AgentComposerCard.swift`,
      note: `the headline and the secondary field are there, but the composer is a pushed page, not a dialog`,
    },
    android: {
      state: `leftover`,
      symbol: `AgentComposerHeadline`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/agent/AgentComposer.kt`,
      note: `same: the headline leads the Agent screen, and the phone presents the launcher as a screen`,
    },
  },
  render: () =>
    [
      `<div class="cmp-stack">`,
      // An ACTION: one chip, and the verb the contract spells `Run`.
      launcher({
        caption: `An action — one chip, and the send is the only thing left to do.`,
        verb: composerUi.runHeadline,
        chips: chip(`Fix merge conflicts`, svgGitMerge),
        account: `claude · danny@exponential.dev`,
        model: `Sonnet 4.6`,
      }),
      // A BATCH: two issue chips, one branch, one combined PR.
      launcher({
        caption: `Two issues — one batch run on one branch, one combined PR.`,
        verb: composerUi.implementHeadline,
        chips: `${chip(`EXP-1019`, ISSUE_DOT)}${chip(`EXP-1039`, ISSUE_DOT)}`,
        account: `codex · danny@exponential.dev`,
        model: `GPT-5.2 Codex`,
      }),
      `</div>`,
    ].join(``),
}
