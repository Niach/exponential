import { motion } from "motion/react"
import { Check, Minus } from "lucide-react"
import { sectionReveal } from "../lib/animations"
import { linearComparison, type CompareCell } from "../lib/pricing"
import { ExpLogo } from "./icons"

function Cell({ cell, isExp }: { cell: CompareCell; isExp?: boolean }) {
  return (
    <div className={`cmp-cell${isExp ? ` is-exp` : ``}`}>
      <span className="cmp-value">
        {isExp && cell.good ? (
          <Check size={13} strokeWidth={2.6} className="cmp-check" />
        ) : (
          !isExp && <Minus size={13} strokeWidth={2} className="cmp-minus" />
        )}
        {cell.value}
      </span>
      {cell.detail && <span className="cmp-detail">{cell.detail}</span>}
    </div>
  )
}

export function ComparisonTable() {
  return (
    <motion.div className="cmp-table" {...sectionReveal}>
      <div className="cmp-row cmp-head">
        <div className="cmp-label" aria-hidden />
        <div className="cmp-cell is-exp cmp-head-cell">
          <ExpLogo size={16} /> Exponential
        </div>
        <div className="cmp-cell cmp-head-cell">Linear</div>
      </div>
      {linearComparison.map((row) => (
        <div className="cmp-row" key={row.label}>
          <div className="cmp-label">{row.label}</div>
          <Cell cell={row.exponential} isExp />
          <Cell cell={row.linear} />
        </div>
      ))}
      <p className="cmp-footnote">
        Linear pricing and features as published at linear.app, June 2026,
        billed yearly. Linear is a trademark of Linear Orbit, Inc., which is
        not affiliated with Exponential.
      </p>
    </motion.div>
  )
}
