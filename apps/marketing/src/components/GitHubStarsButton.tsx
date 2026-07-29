/* "View on GitHub" with a live star-count chip (EXP-337). Two variants:
   `hero` — a ghost button with the count in a glass pill that fades in
   without layout jump; `compact` — the topbar icon button (.topbar-gh, NOT
   .btn-ghost: responsive.css hides `.topbar-right .btn-ghost` ≤420px and
   the GitHub mark must survive there). SSR renders no count (hook initial
   state is null), so there is never a hydration mismatch. */
import { formatStars, useGitHubStars } from "../lib/use-github-stars"
import { LINKS } from "../lib/links"
import { IcGithub } from "./icons"

const Star = ({ size = 11 }: { size?: number }) => (
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="currentColor"
    aria-hidden
    style={{ display: `block`, flexShrink: 0 }}
  >
    <path d="M12 2.6l2.86 5.8 6.4.93-4.63 4.51 1.09 6.37L12 17.2l-5.72 3.01 1.09-6.37L2.74 9.33l6.4-.93L12 2.6z" />
  </svg>
)

export function GitHubStarsButton({
  variant = `hero`,
}: {
  variant?: `hero` | `compact`
}) {
  const stars = useGitHubStars()
  const count = stars === null ? null : formatStars(stars)

  if (variant === `compact`) {
    return (
      <a
        className={`topbar-gh`}
        href={LINKS.github.repo}
        aria-label={`View Exponential on GitHub`}
      >
        <IcGithub size={15} />
        <span className={`topbar-gh-count${count ? ` is-on` : ``}`}>
          {count ?? ``}
        </span>
      </a>
    )
  }

  return (
    <a className={`btn btn-ghost`} href={LINKS.github.repo}>
      <IcGithub size={14} />
      View on GitHub
      <span className={`gh-stars${count ? ` is-on` : ``}`} aria-hidden={!count}>
        <Star />
        {count ?? ``}
      </span>
    </a>
  )
}
