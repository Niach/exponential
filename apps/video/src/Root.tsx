import "./index.css"
import { Composition } from "remotion"
import { WebUiDemo } from "./Video"

export const RemotionRoot: React.FC = () => {
  return (
    <Composition
      id="WebUiDemo"
      component={WebUiDemo}
      durationInFrames={590}
      fps={30}
      width={1920}
      height={1080}
    />
  )
}
