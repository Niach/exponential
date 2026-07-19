/* The lazy Remotion chunk — the ONLY marketing module that may import
   remotion or @video (LoopMovie's SSR contract: the prerenderer must never
   see these imports in the static graph). Loaded via React.lazy when the
   loop section scrolls near. */
import { useEffect, useRef, useState } from "react"
import { Player, type CallbackListener, type PlayerRef } from "@remotion/player"
import {
  CHAPTERS,
  ClosedLoop,
  DURATION_IN_FRAMES,
  FPS,
} from "@video/closedloop"
import type { LoopMovieController } from "./LoopMovie"

const chapterIndexForFrame = (frame: number): number => {
  let index = 0
  for (let i = 0; i < CHAPTERS.length; i++) {
    if (frame >= CHAPTERS[i].frame) index = i
  }
  return index
}

export default function LoopMoviePlayer({
  autoPlay,
  onController,
  onChapterChange,
  onReady,
  onPlayingChange,
}: {
  autoPlay: boolean
  onController: (controller: LoopMovieController) => void
  onChapterChange: (index: number) => void
  onReady: () => void
  onPlayingChange: (playing: boolean) => void
}) {
  const playerRef = useRef<PlayerRef>(null)
  const chapterRef = useRef(-1)

  /* This chunk is client-only (React.lazy), so matchMedia is safe in the
     lazy initializer. On phone widths the composition renders its
     screen-space captions 1.5× (EXP-176: the film scales down to ~343px
     there — the captions are the only text that can stay readable). */
  const [small, setSmall] = useState(
    () => window.matchMedia(`(max-width: 720px)`).matches
  )

  useEffect(() => {
    const mq = window.matchMedia(`(max-width: 720px)`)
    const apply = () => setSmall(mq.matches)
    mq.addEventListener(`change`, apply)
    return () => mq.removeEventListener(`change`, apply)
  }, [])

  useEffect(() => {
    const player = playerRef.current
    if (!player) return

    onController({
      seekToChapter: (index) => {
        const chapter = CHAPTERS[index]
        if (chapter) player.seekTo(chapter.frame)
      },
      play: () => player.play(),
      pause: () => player.pause(),
    })
    onReady()

    const handleFrame: CallbackListener<`frameupdate`> = (event) => {
      const index = chapterIndexForFrame(event.detail.frame)
      if (index !== chapterRef.current) {
        chapterRef.current = index
        onChapterChange(index)
      }
    }
    const handlePlay: CallbackListener<`play`> = () => onPlayingChange(true)
    const handlePause: CallbackListener<`pause`> = () => onPlayingChange(false)

    player.addEventListener(`frameupdate`, handleFrame)
    player.addEventListener(`play`, handlePlay)
    player.addEventListener(`pause`, handlePause)
    return () => {
      player.removeEventListener(`frameupdate`, handleFrame)
      player.removeEventListener(`play`, handlePlay)
      player.removeEventListener(`pause`, handlePause)
    }
  }, [onController, onChapterChange, onReady, onPlayingChange])

  return (
    <Player
      ref={playerRef}
      component={ClosedLoop}
      durationInFrames={DURATION_IN_FRAMES}
      fps={FPS}
      compositionWidth={1920}
      compositionHeight={1080}
      inputProps={{ textScale: small ? 1.5 : 1 }}
      autoPlay={autoPlay}
      loop
      controls={false}
      clickToPlay={false}
      style={{ width: `100%`, height: `100%` }}
    />
  )
}
