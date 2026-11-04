import { Component, type ReactNode } from "react"

// #VAPP-251 (fixtures/apps.json A PACKAGE THAT NO LONGER VALIDATES): any throw
// while building or painting an app's surface lands here, not at the root, so
// the shell around it stays. A new `resetKey` (a republished package) tries
// again without a reload. The message goes to the console, never the screen.
interface Props {
  resetKey: string
  fallback: ReactNode
  children: ReactNode
}

interface State {
  resetKey: string
  failed: boolean
}

export class AppSurfaceBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props)
    this.state = { resetKey: props.resetKey, failed: false }
  }
  static getDerivedStateFromProps(props: Props, state: State): State | null {
    return props.resetKey === state.resetKey ? null : { resetKey: props.resetKey, failed: false }
  }
  static getDerivedStateFromError(): Partial<State> {
    return { failed: true }
  }
  componentDidCatch(error: unknown): void {
    console.error(`[apps] the app surface failed`, error)
  }
  render(): ReactNode {
    return this.state.failed ? this.props.fallback : this.props.children
  }
}
