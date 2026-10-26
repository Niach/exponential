/* The home hero's phone (NEW-15): an agent's chat in a phone frame
   (src/ui-demos/AgentChat.tsx: the person's message, the agent's answer and
   its Exponential UI surface, the press on its reply button and the session
   that starts),
   played by IslandFilm. */
import { IslandFilm, type Film } from "./IslandFilm"

const loadChat = async (): Promise<Film> => {
  const { AgentChat, LAST_STEP, frameMs } = await import(`../ui-demos/AgentChat`)
  return { Frame: AgentChat, last: LAST_STEP, frameMs }
}

export function HeroPhone() {
  return (
    <div className={`home-phone`} role="img" aria-label={`An agent's chat on a phone`}>
      <span className={`home-phone-island`} aria-hidden />
      <div className={`home-phone-status`} aria-hidden>
        <span>9:41</span>
      </div>
      <div className={`home-phone-screen`}>
        <IslandFilm id={`home-agent-chat`} load={loadChat} holdMs={6000} />
      </div>
    </div>
  )
}
