import { render } from "@testing-library/react"
import { Markdown, attachmentImageIds, markdownHost } from "./markdown"

const ID = `01e45218-56c7-475c-9057-347943594a7d`

describe(`Markdown (genUI)`, () => {
  it(`never turns raw HTML in the source into markup`, () => {
    const { container } = render(<Markdown source={`<script>alert(1)</script> <img src=x onerror=1>`} />)
    expect(container.querySelector(`script`)).toBeNull()
    expect(container.querySelector(`img`)).toBeNull()
    expect(container.textContent).toContain(`<script>`)
  })
  it(`renders task items as read-only checkboxes`, () => {
    const { container } = render(<Markdown source={`- [x] done\n- [ ] open`} />)
    const boxes = container.querySelectorAll<HTMLInputElement>(`input[type=checkbox]`)
    expect([...boxes].map((b) => b.checked)).toEqual([true, false])
  })
  it(`an attachment picture is its alt text until its signed URL is back`, () => {
    const { container } = render(<Markdown source={`![Contrast sweep](/api/attachments/${ID})`} />)
    expect(container.querySelector(`img`)).toBeNull()
    expect(container.textContent).toContain(`Contrast sweep`)
    expect(attachmentImageIds(`![a](/api/attachments/${ID}) ![b](https://x.dev/b.png)`)).toEqual([ID])
  })
  it(`loads only signed attachment pictures`, () => {
    const host = markdownHost({ [ID]: `https://s3.example/signed` }, `https://app.example`)
    expect(host.mediaRequest?.(`/api/attachments/${ID}`)).toEqual({ url: `https://s3.example/signed`, headers: {} })
    expect(host.mediaRequest?.(`https://example.com/a.png`)).toBeNull()
  })
  it(`renders GFM tables`, () => {
    const { container } = render(<Markdown source={`| a | b |\n| --- | --- |\n| 1 | 2 |`} />)
    expect(container.querySelectorAll(`td`)).toHaveLength(2)
  })
  it(`resolves relative links against the app origin and opens them through the host`, () => {
    const opened: string[] = []
    const { container } = render(<Markdown source={`[x](/t/acme)`} origin="https://app.example" onOpenLink={(u) => opened.push(u)} />)
    const a = container.querySelector(`a`)!
    expect(a.getAttribute(`href`)).toBe(`https://app.example/t/acme`)
    a.click()
    expect(opened).toEqual([`https://app.example/t/acme`])
  })
})
