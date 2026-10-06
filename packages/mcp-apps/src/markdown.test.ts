import { renderMarkdown } from "./markdown"

describe(`renderMarkdown`, () => {
  it(`escapes raw HTML in the source`, () => {
    const html = renderMarkdown(`<script>alert(1)</script> <img src=x onerror=1>`)
    expect(html).not.toContain(`<script`)
    expect(html).not.toContain(`<img`)
    expect(html).toContain(`&lt;script&gt;`)
  })
  it(`renders task items as disabled checkboxes`, () => {
    const html = renderMarkdown(`- [x] done\n- [ ] open`)
    expect(html).toContain(`<li class="exp-task" data-checked="true"><input type="checkbox" disabled checked /> done`)
    expect(html).toContain(`<li class="exp-task" data-checked="false"><input type="checkbox" disabled /> open`)
  })
  it(`renders an attachment image as a placeholder the view resolves`, () => {
    const html = renderMarkdown(`![Contrast sweep](/api/attachments/01e45218-56c7-475c-9057-347943594a7d)`)
    expect(html).toContain(`data-attachment-id="01e45218-56c7-475c-9057-347943594a7d"`)
    expect(html).not.toContain(`<img`)
  })

  it(`renders a foreign image as its alt text`, () => {
    const html = renderMarkdown(`![x](https://example.com/a.png)`)
    expect(html).toContain(`<span class="exp-markdown-image">x</span>`)
  })
  it(`wraps GFM tables in a scroller`, () => {
    const html = renderMarkdown(`| a | b |\n| --- | --- |\n| 1 | 2 |`)
    expect(html).toContain(`<div class="exp-table"><table>`)
  })
  it(`resolves relative links against the app origin`, () => {
    expect(renderMarkdown(`[x](/t/acme)`, `https://app.example`)).toContain(
      `href="https://app.example/t/acme"`
    )
  })
})
