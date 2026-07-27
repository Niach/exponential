import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { OptionDropdownMenu } from "@/components/option-dropdown-menu"
import {
  issuePriorityOptions,
  issueStatusOptions,
  ISSUE_PRIORITY_FALLBACK,
  ISSUE_STATUS_FALLBACK,
  type IssuePriority,
  type IssueStatus,
} from "@/lib/domain"

// REV2-85: the option tables are DISPLAY-ordered, so an unknown/forward-compat
// value must NOT resolve to `options[0]` ("In Progress" / "Urgent") — the
// trigger falls back to the lifecycle start of the vocabulary, exactly like
// `getIssueStatusConfig` / `getIssuePriorityConfig`.
describe(`OptionDropdownMenu unknown-value fallback`, () => {
  it(`renders the status fallback, not the first display-ordered option`, () => {
    render(
      <OptionDropdownMenu
        disabled
        value={`triaged` as IssueStatus}
        fallbackValue={ISSUE_STATUS_FALLBACK}
        options={issueStatusOptions}
        onSelect={vi.fn()}
        renderTrigger={(selected) => <span>{selected.label}</span>}
      />
    )

    expect(screen.getByText(`Backlog`)).toBeDefined()
    expect(screen.queryByText(`In Progress`)).toBeNull()
  })

  it(`renders the priority fallback, not the first display-ordered option`, () => {
    render(
      <OptionDropdownMenu
        disabled
        value={`blocker` as IssuePriority}
        fallbackValue={ISSUE_PRIORITY_FALLBACK}
        options={issuePriorityOptions}
        onSelect={vi.fn()}
        renderTrigger={(selected) => <span>{selected.label}</span>}
      />
    )

    expect(screen.getByText(`No priority`)).toBeDefined()
    expect(screen.queryByText(`Urgent`)).toBeNull()
  })

  it(`renders the fallback when the value is filtered out of the menu`, () => {
    const creatable = issueStatusOptions.filter(
      (option) => option.value !== `duplicate`
    )

    render(
      <OptionDropdownMenu
        disabled
        value={`duplicate`}
        fallbackValue={ISSUE_STATUS_FALLBACK}
        options={creatable}
        onSelect={vi.fn()}
        renderTrigger={(selected) => <span>{selected.label}</span>}
      />
    )

    expect(screen.getByText(`Backlog`)).toBeDefined()
  })
})
