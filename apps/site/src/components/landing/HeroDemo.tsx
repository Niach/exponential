/* The home page's hero: a team app painting itself under a typed prompt,
   all genUI, in genUI's Window. The build renders the finished app (the HTML a crawler and a
   no-JS reader get); hydrated on view, it replays: the prompt types, the
   send turns into a stop, the bars grow day by day, the rows land, the
   send comes back. Reduced motion keeps the finished state. */
import { useEffect, useMemo } from "react"
import { ExponentialSurface, useSurface } from "@exponential_app/eui-react"
import type { HostPlugin } from "@exponential_app/eui-react"
import type { NestedNode } from "@exponential_app/eui"

const PROMPT = `Hours per project, a month at a glance`
const DAYS = [`1`, `2`, `3`, `6`, `7`, `8`, `9`, `10`, `13`, `14`, `15`, `16`, `17`, `20`, `21`, `22`, `23`, `24`, `27`, `28`]
const HOURS = [7.5, 6, 8, 7, 6.5, 8.5, 7, 5, 8, 7.5, 6, 8, 7, 7.5, 8, 6.5, 7, 8.5, 6, 7.5]
const ROWS = [
  { id: `r1`, date: `Oct 28`, hours: 7.5, project: `Website`, activity: `Pricing page review` },
  { id: `r2`, date: `Oct 27`, hours: 6, project: `Mobile`, activity: `Release 0.14 checklist` },
  { id: `r3`, date: `Oct 24`, hours: 8.5, project: `Apps`, activity: `Hours: monthly export` },
  { id: `r4`, date: `Oct 23`, hours: 7, project: `Website`, activity: `Hero copy and shots` },
]

const TREE: NestedNode = {
  id: `hours`,
  component: `Window`,
  props: { title: `app.exponential.at` },
  children: [
    {
      id: `body`,
      component: `Stack`,
      props: { gap: `lg` },
      style: { padding: `$spacing.lg` },
      children: [
        {
          id: `head`,
          component: `Stack`,
          props: { direction: `horizontal`, justify: `between`, align: `center` },
          children: [
            {
              id: `titles`,
              component: `Stack`,
              props: { gap: `xs` },
              children: [
                { id: `title`, component: `Heading`, props: { text: `Hours`, level: `h3` } },
                { id: `month`, component: `Text`, props: { text: `October 2026`, variant: `muted` } },
              ],
            },
            { id: `total`, component: `Badge`, props: { text: { "@path": `/total` }, variant: `secondary` } },
          ],
        },
        { id: `chart`, component: `Chart`, props: { kind: `bar`, categories: DAYS, series: { "@path": `/series` }, max: 10, height: 180 } },
        {
          id: `table`,
          component: `Table`,
          props: {
            columns: [
              { key: `date`, label: `Date` },
              { key: `hours`, label: `Hours`, type: `number` },
              { key: `project`, label: `Project`, type: `badge` },
              { key: `activity`, label: `Activity` },
            ],
            rows: { "@path": `/rows` },
          },
        },
        { id: `ask`, component: `Composer`, props: { value: { "@path": `/prompt` }, busy: { "@path": `/busy` }, placeholder: `Describe an app` } },
      ],
    },
  ],
}

const total = (n: number) => `${HOURS.slice(0, n).reduce((a, b) => a + b, 0)} h`
const FINAL = { prompt: PROMPT, busy: false, series: [{ name: `Hours`, values: HOURS }], rows: ROWS, total: total(HOURS.length) }

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms))

export default function HeroDemo() {
  const surface = useSurface({ surfaceId: `hero`, initial: TREE, data: FINAL })
  const { setData } = surface
  const host = useMemo<HostPlugin>(() => ({ onInput: () => Promise.resolve(), onAction: () => Promise.resolve() }), [])

  useEffect(() => {
    if (matchMedia(`(prefers-reduced-motion: reduce)`).matches) return
    let live = true
    const play = async () => {
      setData(`/prompt`, ``)
      setData(`/series`, [{ name: `Hours`, values: HOURS.map(() => 0) }])
      setData(`/rows`, [])
      setData(`/total`, `0 h`)
      await wait(600)
      for (let i = 1; i <= PROMPT.length && live; i++) {
        setData(`/prompt`, PROMPT.slice(0, i))
        await wait(32)
      }
      await wait(350)
      if (!live) return
      setData(`/busy`, true)
      for (let d = 1; d <= HOURS.length && live; d++) {
        setData(`/series`, [{ name: `Hours`, values: HOURS.map((h, i) => (i < d ? h : 0)) }])
        setData(`/total`, total(d))
        await wait(55)
      }
      for (let r = 1; r <= ROWS.length && live; r++) {
        setData(`/rows`, ROWS.slice(0, r))
        await wait(140)
      }
      if (live) setData(`/busy`, false)
    }
    void play()
    return () => {
      live = false
    }
  }, [setData])

  return <ExponentialSurface id="xui-hero" surface={surface} theme="exponential" mode="dark" host={host} />
}
