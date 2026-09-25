import { useCallback, useEffect, useRef, useState } from "react"
import { contract } from "@exp/domain-contract"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@exp/ui"

import { useSteerConfig } from "@/components/agent-session"
import { LaunchComposer } from "@/components/launch-composer"
import { launchHeadlineText } from "@/components/launch-dialog/launch-headline"
import type { Team } from "@/db/schema"
import { useLaunchComposer } from "@/hooks/use-launch-composer"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import {
  closeLaunchDialog,
  useLaunchDialogSeed,
  type LaunchDialogRequest,
} from "@/lib/launch-dialog-store"
import type { LaunchSeed } from "@/lib/launch-seed"

// EXP-1019: the START-CODING DIALOG — the ONE launcher, opened over wherever
// the click happened instead of navigating away from it.
//
// EXP-825 had made every play button a navigation to the Agent page with the
// subject in the search params. That lost the context the person was in, and
// the composer it landed on looked like a prompt box with a badge: the whole
// point (your action is loaded, press send) was the smallest thing on screen.
// A subject-carrying start opens this dialog instead (`use-open-composer.ts`
// decides; a subjectless chat still gets the Agent page, which is where a
// chat belongs).
//
// It hosts the EXISTING `LaunchComposer` — the same component the Agent page
// renders inline, with the same hook behind it. There is no second composer,
// so the pickers, the options line, the blocked-start gate, the image
// plumbing and the per-subject submit label cannot drift between the two
// surfaces. What the dialog adds is a shell: a title, a scroll body and the
// rule that a started run shuts it — and, until then, that nothing ELSE
// shuts it: the body owns the `useRemoteStart` whose run watch and deadline
// toast would die with it, so a dismissal while a start is pending is
// refused rather than dropping "Waiting for buildbox…" on the floor.

/**
 * Mounted ONCE in the team layout. Renders nothing until something asks for
 * the launcher, so the composer's live queries and device plumbing only spin
 * up while it is open.
 */
export function LaunchDialogHost({ team }: { team: Team | null | undefined }) {
  const seed = useLaunchDialogSeed()
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
  const open = seed !== null && Boolean(team)
  // Reported up by the body: a start was sent and its run has not landed yet.
  // Every dismissal path Radix owns (✕, the overlay, the sheet drag's hidden
  // Close) funnels through `onOpenChange(false)`, so the guard sits here; the
  // body vetoes Escape and outside-pointer at the layer as well, so the
  // gesture does not even flicker the overlay.
  const [pendingStart, setPendingStart] = useState(false)
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !pendingStart) closeLaunchDialog()
      }}
    >
      {/* Keyed by seed identity: a second play button while the dialog is up
          re-opens it on the new subject rather than merging into the old
          draft. The body mounts only while open — the hook wires the team's
          collections, the device list and the MCP rows.
          Without a relay there is nothing to launch on: the dialog still
          opens and says so, rather than the button doing nothing at all. */}
      {open && team ? (
        steerEnabled ? (
          <LaunchDialogBody
            key={seedKey(seed)}
            team={team}
            request={seed}
            onPendingStartChange={setPendingStart}
          />
        ) : (
          <DialogContent mobile="alert" data-testid="launch-dialog">
            <DialogHeader>
              <DialogTitle>{contract.composerUi.dialogTitle}</DialogTitle>
            </DialogHeader>
            <p className="text-sm text-muted-foreground">
              Live steering is unavailable on this instance.
            </p>
          </DialogContent>
        )
      ) : null}
    </Dialog>
  )
}

/** One mount per opening — the seed IS the identity of a launch. */
function seedKey(seed: LaunchSeed): string {
  return [
    seed.actionId ?? ``,
    seed.issueIds.join(`,`),
    seed.prIssueId ?? ``,
    seed.workflowId ?? ``,
    seed.deviceId ?? ``,
    seed.icon ?? ``,
  ].join(`|`)
}

function LaunchDialogBody({
  team,
  request,
  onPendingStartChange,
}: {
  team: Team
  request: LaunchDialogRequest
  /** `true` from send until the run's row lands or the deadline passes. */
  onPendingStartChange: (pending: boolean) => void
}) {
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id
  const { users } = useTeamUsers(team.id)
  // EXP-870: the ORIGIN the play button named rides the request (a pinned
  // row's `null` = context-free); the watch that opens the run hands it on.
  // The key absent = the hook derives it from the URL, so it is only spread
  // in when the caller answered.
  const { origin, ...seed } = request
  const remote = useRemoteStart({
    enabled: true,
    currentUserId,
    teamId: team.id,
    ...(`origin` in request ? { origin } : {}),
  })
  // The seed is one-shot here as well: the hook reports back once it has
  // applied it, and re-handing it would re-check issues the person just
  // removed. The dialog's own identity (`seedKey`) is what a REopening
  // changes.
  const seededRef = useRef<LaunchSeed | null>(seed)
  const onSeedConsumed = useCallback(() => {
    seededRef.current = null
  }, [])
  const model = useLaunchComposer({
    teamId: team.id,
    remote,
    seed: seededRef.current,
    onSeedConsumed,
  })

  // The launcher gets out of the way once its run is under way — but not a
  // frame earlier: the WATCH that navigates to the run (and the deadline that
  // says the machine refused it) lives in this `useRemoteStart`, so unmounting
  // at send time would drop both and leave "Waiting for buildbox…" nowhere.
  // `sentTo` is set on send and cleared when the run's row lands (the
  // navigation) or the deadline passes (the toast) — the FALLING edge is the
  // moment the dialog is done.
  const sent = remote.sentTo !== null
  const wasSentRef = useRef(false)
  useEffect(() => {
    onPendingStartChange(sent)
    if (sent) {
      wasSentRef.current = true
      return
    }
    if (wasSentRef.current) closeLaunchDialog()
  }, [sent, onPendingStartChange])
  // A remount on a new seed must not leave the host thinking a start is
  // still pending.
  useEffect(() => () => onPendingStartChange(false), [onPendingStartChange])

  return (
    <DialogContent
      mobile="sheet-full"
      className="flex max-h-[85vh] flex-col gap-0 sm:max-w-2xl"
      data-testid="launch-dialog"
      onEscapeKeyDown={(event) => {
        if (sent) event.preventDefault()
      }}
      onInteractOutside={(event) => {
        if (sent) event.preventDefault()
      }}
    >
      <DialogHeader>
        {/* The VISIBLE headline is the composer's own (chips and all); the
            dialog's accessible name is the same line as plain text, so a
            screen reader hears "Run Fix merge conflicts" rather than a
            generic title. */}
        <DialogTitle className="sr-only">
          {model.subject === null
            ? contract.composerUi.dialogTitle
            : launchHeadlineText(model)}
        </DialogTitle>
      </DialogHeader>
      <DialogBody className="pb-1">
        <LaunchComposer model={model} users={users} />
      </DialogBody>
    </DialogContent>
  )
}
