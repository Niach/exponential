import { useCallback, useRef, useState } from "react"
import type {
  ComposerDraftField,
  ComposerTextUpdate,
} from "@/hooks/use-composer-draft"

// #FEED-219: a test double for `hooks/use-composer-draft` — a plain field
// with no synced row, for suites that render a composer but test something
// else (they mock the collections without the drafts shape). Use:
// vi.mock(`@/hooks/use-composer-draft`, () => import(`@/test/composer-draft-plain`))

export function useComposerDraft(): ComposerDraftField {
  const [text, setState] = useState(``)
  const textRef = useRef(text)
  textRef.current = text
  const setText = useCallback((next: ComposerTextUpdate) => {
    const value = typeof next === `function` ? next(textRef.current) : next
    textRef.current = value
    setState(value)
  }, [])
  return { text, textRef, setText }
}

export function useComposerDraftKeys(): ReadonlySet<string> {
  return new Set()
}
