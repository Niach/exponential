import { useCallback, useEffect, useState } from "react"
import { Button, DEVICE_READINESS_COPY, GlassGroup, GlassSectionHeader, GlassToggleRow, ListRow, Prompt, SETTINGS_LIST_CLASS } from "@exp/ui"
import { MEMBER_SPACE, MEMBER_SPACE_COPY, mayOptIn } from "@exp/domain-contract/member-space"
import type { DeviceDoctor, Team } from "@/db/schema"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { memberSpaceOptInPrompt, promptActions } from "@/lib/prompts"
import type { SteerDevice } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"

// #EXP-2161 (member-space.json, UI ×4): members' Assistants on a shared
// device. The owner: under each shared team's switch the `copy.optIn` row
// (disabled with the doctor's detail while it cannot), turning on confirms
// (`copy.optIn*`). A team owner: the same switch, off only, on a device they
// do not own. Both: one `copy.members` row per member with a space here.

/** The opt-in row's state (`optInCases`): off is always allowed; `detail`
 *  = why the device cannot (its doctor item's). */
export function optInRow(input: { checked: boolean; kind: string; doctor: DeviceDoctor | null | undefined }): {
  disabled: boolean
  detail: string | null
} {
  const item = input.doctor?.items.find((entry) => entry.key === MEMBER_SPACE.doctorKey)
  const able = mayOptIn({ caller: `deviceOwner`, allowed: true, kind: input.kind, shared: true, doctor: item?.state ?? null })
  return {
    disabled: !able && !input.checked,
    detail: able ? null : (item?.detail ?? DEVICE_READINESS_COPY.unavailable),
  }
}

/** The owner's opt-in row of one shared team (inside the Sharing group). */
export function MemberSpaceOptInRow({
  deviceId,
  teamId,
  kind,
  doctor,
  checked,
  onChanged,
}: {
  deviceId: string
  teamId: string
  kind: string
  doctor: DeviceDoctor | null | undefined
  checked: boolean
  /** The opt-in moved: members may have ended with it. */
  onChanged?: () => void
}) {
  const [confirming, setConfirming] = useState(false)
  const [busy, setBusy] = useState(false)
  const row = optInRow({ checked, kind, doctor })
  const set = async (allowed: boolean): Promise<void> => {
    if (busy) return
    setBusy(true)
    try {
      await trpc.devices.setAssistantTeam.mutate({ deviceId, teamId, allowed }, SKIP_ERROR_TOAST)
      setConfirming(false)
      onChanged?.()
    } catch (error) {
      failedAction(`update`, `sharing`, error)
    } finally {
      setBusy(false)
    }
  }
  const copy = memberSpaceOptInPrompt()
  return (
    <>
      <GlassToggleRow
        id={`device-settings-assistants-${teamId}`}
        label={MEMBER_SPACE_COPY.optIn}
        description={row.disabled ? row.detail : undefined}
        checked={checked}
        onCheckedChange={(next) => (next ? setConfirming(true) : void set(false))}
        disabled={row.disabled || busy}
        className="pl-8"
      />
      <Prompt
        open={confirming}
        onOpenChange={setConfirming}
        busy={busy}
        title={copy.title}
        body={copy.body}
        actions={promptActions(copy, { allow: { busy, onSelect: () => set(true), testId: `member-space-allow` } })}
      />
    </>
  )
}

type SpaceMember = { spaceId: string; userId: string; name: string; teamId: string | null }

/** How often the open list is read again (the spaces are not synced to it). */
const MEMBERS_INTERVAL_MS = 30_000

/** The members with a space on a device; `version` = anything that may end one. */
function useSpaceMembers(rowId: string | undefined, version: string) {
  const [members, setMembers] = useState<SpaceMember[]>([])
  const load = useCallback(async () => {
    if (!rowId) return
    const answer = await trpc.memberSpaces.forDevice.query({ id: rowId }, SKIP_ERROR_TOAST).catch(() => null)
    if (answer) setMembers(answer.members)
  }, [rowId])
  useEffect(() => {
    setMembers([])
  }, [rowId])
  useEffect(() => {
    void load()
    const timer = setInterval(() => void load(), MEMBERS_INTERVAL_MS)
    return () => clearInterval(timer)
  }, [load, version])
  return { members, reload: load }
}

/** `copy.members`: who runs their Assistant here, each with `copy.remove`. */
export function MemberSpaceMembers({ rowId, version = `` }: { rowId: string | undefined; version?: string }) {
  const { members, reload } = useSpaceMembers(rowId, version)
  const [removing, setRemoving] = useState<string | null>(null)
  const remove = async (spaceId: string) => {
    if (removing) return
    setRemoving(spaceId)
    try {
      await trpc.memberSpaces.remove.mutate({ spaceId }, SKIP_ERROR_TOAST)
      await reload()
    } catch (error) {
      failedAction(`remove`, `the member`, error)
    } finally {
      setRemoving(null)
    }
  }
  if (members.length === 0) return null
  return (
    <div data-testid="member-spaces">
      <GlassSectionHeader label={MEMBER_SPACE_COPY.members} />
      <div className={SETTINGS_LIST_CLASS}>
        {members.map((member) => (
          <ListRow key={member.spaceId} data-testid={`member-space-${member.spaceId}`}>
            <span className="min-w-0 flex-1 truncate text-sm">{member.name}</span>
            <Button
              variant="ghost"
              size="sm"
              pending={removing === member.spaceId}
              disabled={removing !== null}
              onClick={() => void remove(member.spaceId)}
            >
              {MEMBER_SPACE_COPY.remove}
            </Button>
          </ListRow>
        ))}
      </div>
    </div>
  )
}

/** A team owner switches members' Assistants off on a teammate's device. */
export async function dropAssistantTeam(rowId: string, teamId: string): Promise<void> {
  try {
    await trpc.devices.dropAssistantTeam.mutate({ id: rowId, teamId }, SKIP_ERROR_TOAST)
  } catch (error) {
    failedAction(`update`, `sharing`, error)
  }
}

/** The device page of a machine the viewer does not own: for each team they
 *  own that is opted in, the off switch; then the members running through it. */
export function TeamOwnerSpaces({ device, ownedTeams }: { device: SteerDevice; ownedTeams: readonly Pick<Team, `id` | `name`>[] }) {
  const rowId = device.rowId
  const opted = ownedTeams.filter((team) => device.assistantTeamIds?.includes(team.id))
  const [busy, setBusy] = useState(false)
  if (!rowId || opted.length === 0) return null
  return (
    <section className="space-y-2" data-testid="device-team-owner-spaces">
      {opted.map((team) => (
        <div key={team.id}>
          <GlassSectionHeader label={team.name} />
          <GlassGroup>
            <GlassToggleRow
              id={`device-assistants-${team.id}`}
              label={MEMBER_SPACE_COPY.optIn}
              checked
              disabled={busy}
              onCheckedChange={() => {
                setBusy(true)
                void dropAssistantTeam(rowId, team.id).finally(() => setBusy(false))
              }}
            />
          </GlassGroup>
        </div>
      ))}
      <MemberSpaceMembers rowId={rowId} version={opted.map((team) => team.id).join(`,`)} />
    </section>
  )
}
