import { useState, type ReactNode } from "react"
import {
  Button,
  CHIP_GLYPH_CLASS,
  EntityRefChipView,
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
  MobilePopover,
  MobilePopoverContent,
  MobilePopoverTrigger,
  StatusGlyph,
  conceptIcon,
  useIsMobile,
} from "@exp/ui"
import { entityChipLabel } from "@exp/domain-contract/entity-preview"
import type { EntityRef } from "@/lib/mcp/preview"
import type { ResolvedIssueRef } from "@/components/issue-ref-provider"
import { statusColorClass } from "@/components/issue-properties/status-dropdown"
import { useTeamStatusesContext } from "@/hooks/use-team-statuses"
import { EntityRefPreviewCard } from "./entity-preview-card"
import { useEntityRefRow } from "./use-entity-ref-row"
import { EntityRefLink, useEntityRefTarget } from "./use-entity-ref-target"

// EXP-920: ONE chip in a tool row — the @exp/ui `EntityChip` bound to this
// client's rows. It resolves three things and nothing else:
//
//   * the GLYPH: an issue's resolved status glyph (the very glyph its
//     `#IDENT` chip draws), every other kind its concept;
//   * the TARGET (`useEntityRefTarget`): a real router link around the body,
//     so ⌘-click and copy-address work; a `list` chip never navigates;
//   * the CARD: on a pointer device a hover card (the issue preview's 400/100
//     ms pair), on a phone a tap opens a bottom sheet with the same card and
//     an "Open" row — a preview that opened under the finger would just
//     swallow the tap (EXP-760).
//
// An unsynced row draws the same chip muted, with no target and no card:
// the answer named something this viewer cannot see (another team's board,
// a row deleted since).

const OpenIcon = conceptIcon(`ui-chevron-right`)

/** Whether `EntityRefPreviewCard` has anything to draw for a synced ref: a
 *  list with no members and a row-less kind with no title have nothing, and
 *  an empty hover surface must not open. Exported for the test. */
export function cardExists(
  entityRef: EntityRef,
  members: readonly EntityRef[]
): boolean {
  if (entityRef.kind === `list`) return members.length > 0
  if (entityRef.kind === `repository` || entityRef.kind === `thread`) {
    return Boolean(entityRef.title?.trim())
  }
  return true
}

export function EntityRefChip({
  entityRef,
  members,
  className,
  testId,
}: {
  entityRef: EntityRef
  /** A `list` ref's absorbed member refs (`groupPreviewRefs`). */
  members: readonly EntityRef[]
  className?: string
  testId?: string
}): ReactNode {
  const isMobile = useIsMobile()
  const { row, synced } = useEntityRefRow(entityRef)
  const { target, link } = useEntityRefTarget(entityRef)
  const { resolve } = useTeamStatusesContext()
  const [open, setOpen] = useState(false)

  const label = entityChipLabel(entityRef)
  // An issue chip paints its resolved status once the row is here; the
  // unsynced one keeps a muted dashed glyph standing in for the status this
  // client cannot resolve yet (EXP-887).
  let icon: ReactNode | undefined
  if (entityRef.kind === `issue`) {
    const issue = row as ResolvedIssueRef | null
    const status = issue ? resolve(issue) : null
    icon = status ? (
      <StatusGlyph
        icon={status.icon}
        colorClass={statusColorClass(status)}
        colorHex={status.builtinKey ? undefined : status.colorHex}
        className={CHIP_GLYPH_CLASS}
      />
    ) : (
      <StatusGlyph
        icon="circle-dashed"
        colorClass="text-muted-foreground"
        className={CHIP_GLYPH_CLASS}
      />
    )
  }

  const muted = !synced
  const hasCard = synced && cardExists(entityRef, members)
  const chipTestId = testId ?? `entity-chip-${entityRef.kind}`

  if (muted) {
    return (
      <EntityRefChipView
        entityRef={entityRef}
        icon={icon}
        muted
        className={className}
        testId={chipTestId}
      />
    )
  }

  if (isMobile) {
    // The chip IS the sheet's trigger (its body becomes the toggle); the
    // sheet carries the card and the one row that navigates.
    if (!hasCard && !target) {
      return (
        <EntityRefChipView
          entityRef={entityRef}
          icon={icon}
          className={className}
          testId={chipTestId}
        />
      )
    }
    if (!hasCard && link) {
      return (
        <EntityRefChipView
          entityRef={entityRef}
          icon={icon}
          link={link}
          className={className}
          testId={chipTestId}
        />
      )
    }
    return (
      <MobilePopover open={open} onOpenChange={setOpen}>
        <MobilePopoverTrigger asChild>
          <EntityRefChipView
            entityRef={entityRef}
            icon={icon}
            className={className}
            testId={chipTestId}
          />
        </MobilePopoverTrigger>
        <MobilePopoverContent
          mobileTitle={label}
          className="w-80 p-3"
          data-testid="entity-preview-sheet"
        >
          <div className="flex flex-col gap-3 px-4 pb-4">
            <EntityRefPreviewCard entityRef={entityRef} members={members} />
            {target && (
              <Button asChild variant="glass" size="sm" className="justify-between">
                <EntityRefLink route={target} onClick={() => setOpen(false)}>
                  {`Open ${label}`}
                  <OpenIcon className="size-4" />
                </EntityRefLink>
              </Button>
            )}
          </div>
        </MobilePopoverContent>
      </MobilePopover>
    )
  }

  const chip = (
    <EntityRefChipView
      entityRef={entityRef}
      icon={icon}
      link={link}
      className={className}
      testId={chipTestId}
    />
  )
  if (!hasCard) return chip
  return (
    <HoverCard>
      <HoverCardTrigger asChild>{chip}</HoverCardTrigger>
      <HoverCardContent data-testid="entity-preview-hover">
        <EntityRefPreviewCard entityRef={entityRef} members={members} />
      </HoverCardContent>
    </HoverCard>
  )
}
