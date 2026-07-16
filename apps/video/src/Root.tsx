import "./index.css"
import { Composition } from "remotion"
import { WebUiDemo } from "./Video"
import { ShipsItsOwnIssues } from "./ships/ShipsItsOwnIssues"
import { LaunchSpot } from "./spot/LaunchSpot"
import { LaunchSpotVertical } from "./spot-vertical/LaunchSpotVertical"
import { ClosedLoop, DURATION_IN_FRAMES as CLOSED_LOOP_FRAMES, FPS as CLOSED_LOOP_FPS } from "./closedloop"

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="ClosedLoop"
        component={ClosedLoop}
        durationInFrames={CLOSED_LOOP_FRAMES}
        fps={CLOSED_LOOP_FPS}
        width={1920}
        height={1080}
      />
      <Composition
        id="LaunchSpot"
        component={LaunchSpot}
        durationInFrames={1080}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="LaunchSpotVertical"
        component={LaunchSpotVertical}
        durationInFrames={450}
        fps={30}
        width={1080}
        height={1920}
      />
      <Composition
        id="ShipsItsOwnIssues"
        component={ShipsItsOwnIssues}
        durationInFrames={1500}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="WebUiDemo"
        component={WebUiDemo}
        durationInFrames={590}
        fps={30}
        width={1920}
        height={1080}
      />
    </>
  )
}
