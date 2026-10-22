import { fireEvent, render } from "@solidjs/testing-library"
import { describe, expect, it, vi } from "vitest"

import { CodeBlock, CodeBlockActions, CodeBlockContent, CodeBlockCopyButton, CodeBlockHeader, CodeBlockTitle } from "./code-block"

// EXP-1324: shadcn's code-block, coloured by the catalog tokenizer (React's
// code-block.test.tsx).

const slot = (root: HTMLElement, name: string) => root.querySelector(`[data-slot="${name}"]`) as HTMLElement | null

describe(`CodeBlock`, () => {
  it(`stamps variant and size`, () => {
    const { container } = render(() => (
      <CodeBlock size="sm" variant="destructive">
        <CodeBlockContent>x</CodeBlockContent>
      </CodeBlock>
    ))
    const root = slot(container, `code-block`)!
    expect(root.getAttribute(`data-size`)).toBe(`sm`)
    expect(root.getAttribute(`data-variant`)).toBe(`destructive`)
    expect(root.className).toContain(`border-destructive/30`)
  })

  it(`tokenizes string children per line in the language`, () => {
    const { container } = render(() => (
      <CodeBlock>
        <CodeBlockContent language="ts">{`const a = 1\nreturn a\n`}</CodeBlockContent>
      </CodeBlock>
    ))
    const lines = container.querySelectorAll(`[data-slot="code-block-line"]`)
    expect(lines).toHaveLength(2)
    const keyword = container.querySelector(`[data-token="keyword"]`)!
    expect(keyword.textContent).toBe(`const`)
    expect(keyword.className).toContain(`text-(--hljs-keyword)`)
    expect(keyword.className).toContain(`group-data-[variant=destructive]/code-block:text-current`)
  })

  it(`colours command output by its line kinds`, () => {
    const { container } = render(() => <CodeBlockContent language="ansi">{`> Task :app:startupProfile\nBUILD SUCCESSFUL in 41s`}</CodeBlockContent>)
    expect(container.querySelector(`[data-token="comment"]`)!.textContent).toBe(`> Task :app:startupProfile`)
    expect(container.querySelector(`[data-token="success"]`)!.textContent).toBe(`BUILD SUCCESSFUL in 41s`)
  })

  it(`keeps blank lines and caps the height at maxLines`, () => {
    const { container } = render(() => (
      <CodeBlockContent language="plain" maxLines={4}>
        {`a\n\nb`}
      </CodeBlockContent>
    ))
    const lines = container.querySelectorAll(`[data-slot="code-block-line"]`)
    expect(lines).toHaveLength(3)
    expect(lines[1]!.textContent).toBe(`\n`)
    expect(slot(container, `code-block-content`)!.style.maxHeight).toContain(`4lh`)
  })

  it(`copies and says so for two seconds`, async () => {
    vi.useFakeTimers()
    const writeText = vi.fn(() => Promise.resolve())
    Object.assign(navigator, { clipboard: { writeText } })
    const { getByRole } = render(() => (
      <CodeBlock size="sm">
        <CodeBlockHeader>
          <CodeBlockTitle>Output</CodeBlockTitle>
          <CodeBlockActions>
            <CodeBlockCopyButton value="BUILD SUCCESSFUL" />
          </CodeBlockActions>
        </CodeBlockHeader>
        <CodeBlockContent>BUILD SUCCESSFUL</CodeBlockContent>
      </CodeBlock>
    ))
    const button = getByRole(`button`)
    expect(button.getAttribute(`aria-label`)).toBe(`Copy`)
    fireEvent.click(button)
    await vi.advanceTimersByTimeAsync(0)
    expect(writeText).toHaveBeenCalledWith(`BUILD SUCCESSFUL`)
    expect(button.getAttribute(`aria-label`)).toBe(`Copied`)
    vi.advanceTimersByTime(2000)
    expect(button.getAttribute(`aria-label`)).toBe(`Copy`)
    vi.useRealTimers()
  })
})
