/* The post list: one row per post, title and date, nothing else. The index,
   a post's footer and the home strip share it. */
import { formatDate, postPath, type PostMeta } from "../../blog/types"

export function PostList({ posts }: { posts: PostMeta[] }) {
  return (
    <ul className="blog-rows">
      {posts.map((meta) => (
        <li key={meta.slug}>
          <a className="blog-row" href={postPath(meta.slug)}>
            <span className="blog-row-title">{meta.title}</span>
            <span className="blog-row-meta">
              <time dateTime={meta.date}>{formatDate(meta.date)}</time> · {meta.readingMinutes} min
            </span>
          </a>
        </li>
      ))}
    </ul>
  )
}
