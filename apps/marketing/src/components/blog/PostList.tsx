/* The post list: one row per post, a small cover thumbnail, the title and
   the day. `byYear` groups the rows under a year label (the index); the
   home list and a post's footer show a flat list. */
import { formatMonthDay, postPath, type PostMeta } from "../../blog/types"

function Rows({ posts }: { posts: PostMeta[] }) {
  return (
    <ul className="blog-rows">
      {posts.map((meta) => (
        <li key={meta.slug}>
          <a className="blog-row" href={postPath(meta.slug)}>
            <img className="blog-row-thumb" src={meta.cover.src} alt="" width={meta.cover.width} height={meta.cover.height} loading="lazy" decoding="async" />
            <span className="blog-row-title">{meta.title}</span>
            <time className="blog-row-date" dateTime={meta.date}>
              {formatMonthDay(meta.date)}
            </time>
          </a>
        </li>
      ))}
    </ul>
  )
}

export function PostList({ posts, byYear = false }: { posts: PostMeta[]; byYear?: boolean }) {
  if (!byYear) return <Rows posts={posts} />
  const years = [...new Set(posts.map((meta) => meta.date.slice(0, 4)))]
  return (
    <>
      {years.map((year) => (
        <section key={year} className="blog-year">
          <h2 className="blog-year-label">{year}</h2>
          <Rows posts={posts.filter((meta) => meta.date.startsWith(year))} />
        </section>
      ))}
    </>
  )
}
