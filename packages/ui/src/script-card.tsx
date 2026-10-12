import { useState } from "react"
import type { ScriptCard as ScriptCardModel } from "@exp/domain-contract/script-card"
import fixture from "@exp/domain-contract/fixtures/script-card.json"

import { cn } from "./cn"
import { ExponentialLogo } from "./exponential-logo"
import { Spinner } from "./feedback"
import { conceptIcon } from "./icons.generated"

// EXP-1279 (EXP-1259 D13): a code-mode call (`mcp__codemode__exec`, contract
// toolKind `script`) as ONE card: "Ran a script · 59 calls · 3.0 s", the
// calls it made grouped by tool (our brand mark on an Exponential tool, the
// generic tool glyph otherwise; first subject + "+n", ×count, failures in
// rose) and the script itself behind the "Script" disclosure. The model is
// the contract's (`scriptCard()`, fixture `script-card.json` ×4); this only
// paints it.

const TEST_IDS = fixture.testIds

const ScriptIcon = conceptIcon(`tool-script`)
const ToolIcon = conceptIcon(`coding-tool`)
const ChevronIcon = conceptIcon(`ui-chevron-right`)

export function ScriptCard({
  card,
  running = false,
  failed = false,
  source,
  className,
}: {
  /** `scriptCard(input)` from `@exp/domain-contract/script-card`. */
  card: ScriptCardModel
  /** The call is still going: the title alone with the run's spinner. */
  running?: boolean
  /** The exec call itself failed (not one of its calls). */
  failed?: boolean
  /** The exec input's `script`; absent = no disclosure. */
  source?: string | null
  className?: string
}) {
  const [open, setOpen] = useState(false)
  const script = source?.trim() ? source : null
  return (
    <div
      data-testid={TEST_IDS.card}
      className={cn(
        `min-w-0 overflow-clip rounded-lg border border-glass-stroke-card bg-glass-row text-xs`,
        className
      )}
    >
      <div className="flex h-10 min-w-0 items-center gap-2 px-3">
        <ScriptIcon
          className={cn(
            `size-3.5 shrink-0`,
            failed ? `text-rose-400` : `text-muted-foreground`
          )}
        />
        <span
          className={cn(`shrink-0 font-medium`, failed && `text-rose-400`)}
        >
          {card.title}
        </span>
        {running && <Spinner size={12} label={card.title} />}
        {card.meta && (
          <span className="-ml-1 min-w-0 truncate tabular-nums text-muted-foreground">
            {`· ${card.meta}`}
          </span>
        )}
        {script !== null && (
          <button
            type="button"
            aria-expanded={open}
            className="ml-auto flex shrink-0 items-center gap-1 rounded-sm text-muted-foreground hover:text-foreground"
            onClick={() => setOpen((value) => !value)}
          >
            {fixture.copy.script}
            <ChevronIcon
              className={cn(
                `size-3.5 transition-transform duration-fast ease-standard motion-reduce:transition-none`,
                open && `rotate-90`
              )}
            />
          </button>
        )}
      </div>
      {open && script !== null && (
        <pre
          data-testid={TEST_IDS.source}
          className="mx-3 mb-2 max-h-72 overflow-auto overscroll-contain whitespace-pre-wrap break-words rounded-md border border-glass-stroke px-2 py-1.5 font-mono text-[0.6875rem] leading-relaxed text-muted-foreground"
        >
          {script}
        </pre>
      )}
      {(card.groups.length > 0 || card.overflow) && (
        <div className="pb-1.5">
          {card.groups.map((group) => (
            <div
              key={group.tool}
              data-testid={TEST_IDS.group}
              className="flex h-7 min-w-0 items-center gap-2 px-3"
            >
              {group.exponential ? (
                <ExponentialLogo
                  variant="light"
                  size={12}
                  className="size-3 shrink-0 text-muted-foreground"
                />
              ) : (
                <ToolIcon className="size-3 shrink-0 text-muted-foreground" />
              )}
              <span className="shrink-0">{group.label}</span>
              {group.subject && (
                <span
                  className="min-w-0 truncate text-muted-foreground"
                  title={group.subject}
                >
                  {group.more ? `${group.subject} ${group.more}` : group.subject}
                </span>
              )}
              {(group.failed || group.count) && (
                <span
                  className={cn(
                    `ml-auto shrink-0 tabular-nums`,
                    group.failed ? `text-rose-400` : `text-muted-foreground`
                  )}
                >
                  {group.failed ?? group.count}
                </span>
              )}
            </div>
          ))}
          {card.overflow && (
            <div className="flex h-7 min-w-0 items-center px-3 text-muted-foreground">
              {card.overflow}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
