// FEED-131: the `plain` lane's primitive specimens (React = the reference).
// One entry per `primitive-specimens.json` row whose `primitive` is "plain";
// the SAME ids in packages/exponential-ui-solid/harness/primitives/plain.tsx.
// Layout is inline style (the shipped sheet only holds the primitives'
// utilities); every specimen is static.
import type { CSSProperties, ReactElement } from "react"
import { CircleAlert, Inbox, Plus, Sparkles } from "lucide-react"
import { DockCard, DockPile } from "../../src/primitives/dock-card"
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Badge,
  Breadcrumb,
  BreadcrumbEllipsis,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
  Button,
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
  ChoiceTiles,
  CodeBlock,
  CodeBlockActions,
  CodeBlockContent,
  CodeBlockCopyButton,
  CodeBlockFooter,
  CodeBlockHeader,
  CodeBlockLine,
  CodeBlockTitle,
  ContextRing,
  DisclosureHeader,
  EmptyCta,
  EmptyState,
  IconDisc,
  Input,
  Kbd,
  KbdGroup,
  Label,
  ListEmpty,
  Meter,
  Progress,
  ProgressRing,
  Separator,
  Skeleton,
  Spinner,
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
  VoiceWave,
} from "../../src/primitives"

const ROW: CSSProperties = { display: `flex`, flexWrap: `wrap`, alignItems: `center`, gap: 8 }
const COLUMN: CSSProperties = { display: `flex`, flexDirection: `column`, gap: 12 }
const VARIANTS = [`default`, `destructive`, `outline`, `glass`, `secondary`, `ghost`, `link`, `text`] as const
const TEXT_SIZES = [`default`, `xs`, `sm`, `lg`, `inline`] as const
const ICON_SIZES = [`icon`, `icon-sm`, `icon-xs`] as const
const PICTURE = `data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 10'%3E%3Crect width='16' height='10' fill='%2360a5fa'/%3E%3C/svg%3E`
const SOURCE = `import { tokenizeCode } from "@exponential_app/ui"\n\nconst lines = tokenizeCode(source, \`ts\`)\nexport default lines.length\n`
const noop = () => {}
const DOT: CSSProperties = { display: `block`, borderRadius: 9999, background: `#2563eb` }

export const SPECIMENS: Record<string, () => ReactElement> = {
  "plain/button": () => (
    <div style={COLUMN}>
      {VARIANTS.map((variant) => (
        <div key={variant} style={ROW}>
          {TEXT_SIZES.map((size) => (
            <Button key={size} variant={variant} size={size} data-xui-target={variant === `default` && size === `default` ? `` : undefined}>
              {`${variant} ${size}`}
            </Button>
          ))}
          {ICON_SIZES.map((size) => (
            <Button key={size} variant={variant} size={size} aria-label="Add">
              <Plus />
            </Button>
          ))}
          <Button variant={variant} disabled>
            Disabled
          </Button>
        </div>
      ))}
      <div style={ROW}>
        <Button variant="outline" asChild>
          <a href="#docs">Docs link</a>
        </Button>
        <Button size="sm">
          <Plus /> With icon
        </Button>
        <Button variant="ghost" aria-pressed="true">
          Pressed
        </Button>
        <Button variant="outline" aria-invalid="true">
          Invalid
        </Button>
      </div>
    </div>
  ),
  "plain/badge": () => (
    <div style={ROW}>
      <Badge count={3} />
      <Badge count={5} tone="primary" />
      <Badge count={140} />
      <Badge count={12} max={9} tone="primary" />
      <Badge count={0} />
    </div>
  ),
  "plain/alert": () => (
    <div style={{ ...COLUMN, width: 420 }}>
      <Alert>
        <AlertTitle>Heads up</AlertTitle>
        <AlertDescription>The board moves to the new team on Monday.</AlertDescription>
      </Alert>
      <Alert variant="destructive">
        <CircleAlert />
        <AlertTitle>Couldn't save the board</AlertTitle>
        <AlertDescription>
          <p>The server refused the change.</p>
        </AlertDescription>
      </Alert>
    </div>
  ),
  "plain/card": () => (
    <div style={{ width: 380 }}>
      <Card>
        <CardHeader>
          <CardTitle>Team plan</CardTitle>
          <CardDescription>Three seats, billed yearly.</CardDescription>
          <CardAction>
            <Button variant="outline" size="sm">
              Change
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          <p>Unlimited widgets and 10 GB of storage.</p>
        </CardContent>
        <CardFooter>
          <Button>Upgrade</Button>
        </CardFooter>
      </Card>
    </div>
  ),
  "plain/input-textarea": () => (
    <div style={{ ...COLUMN, width: 360 }}>
      <Label htmlFor="plain-name">Name</Label>
      <Input id="plain-name" placeholder="Your name" data-xui-target="" />
      <Input defaultValue="Ada Lovelace" />
      <Input defaultValue="Disabled" disabled />
      <Input defaultValue="not-an-email" aria-invalid="true" />
      <Textarea placeholder="Describe the bug" />
      <Textarea defaultValue={`Steps:\n1. Open the board`} />
      <Textarea defaultValue="Disabled" disabled />
      <Textarea defaultValue="Too short" aria-invalid="true" />
    </div>
  ),
  "plain/kbd-separator-skeleton-spinner": () => (
    <div style={{ ...COLUMN, width: 320 }}>
      <div style={ROW}>
        <KbdGroup>
          <Kbd>⌘</Kbd>
          <Kbd>K</Kbd>
        </KbdGroup>
        <Kbd>Esc</Kbd>
      </div>
      <Separator />
      <div style={{ ...ROW, height: 24 }}>
        <span>Left</span>
        <Separator orientation="vertical" />
        <span>Right</span>
        <Separator orientation="vertical" decorative={false} />
        <span>Semantic</span>
      </div>
      <Separator decorative={false} />
      <Skeleton style={{ width: 128, height: 16 }} />
      <Skeleton style={{ width: 40, height: 40 }} />
      <div style={ROW}>
        <Spinner />
        <Spinner size={12} label="Starting" />
        <Spinner size={24} />
      </div>
    </div>
  ),
  "plain/empty-state": () => (
    <div style={{ ...COLUMN, width: 480 }}>
      <EmptyState icon={Inbox} title="No issues yet" description="Create the first issue on this board.">
        <Button>New issue</Button>
      </EmptyState>
      <EmptyState icon={Inbox} title="Nothing here" />
      <EmptyState size="list" icon={Inbox} title="No sessions" />
      <EmptyState size="list" title="No matches" />
      <ListEmpty>No emoji found</ListEmpty>
      <EmptyCta icon={Plus} title="No custom actions yet" description="Describe one and your agent will build it" />
    </div>
  ),
  "plain/icon-disc-meter": () => (
    <div style={COLUMN}>
      <div style={ROW}>
        <IconDisc icon={Sparkles} />
        <IconDisc icon={Sparkles} tone="success" />
        <IconDisc icon={Sparkles} tone="danger" />
        <IconDisc icon={Sparkles} tone="muted" strokeWidth={1.5} />
      </div>
      <div style={{ ...COLUMN, width: 240 }}>
        <Meter value={30} />
        <Meter value={67} tone="warning" />
        <Meter value={95} tone="danger" />
        <Meter value={null} />
      </div>
    </div>
  ),
  "plain/progress": () => (
    <div style={{ ...COLUMN, width: 240 }}>
      <Progress value={0} />
      <Progress value={42} />
      <Progress value={100} />
      <Progress value={null} />
    </div>
  ),
  "plain/rings": () => (
    <div style={ROW}>
      <ProgressRing done={0} total={3} />
      <ProgressRing done={2} total={5} />
      <ProgressRing done={5} total={5} size={24} colorHex="#16a34a" />
      <ContextRing percent={null} showEmpty />
      <ContextRing percent={50} />
      <ContextRing percent={80} tone="warning" />
      <ContextRing percent={98} tone="danger" />
      <ContextRing percent={null} />
    </div>
  ),
  // The first four = fixtures/voice-wave.json's `child` cases (in order;
  // browser/voice-wave-child.test.ts measures their drawn child).
  "plain/voice-wave": () => (
    <div style={ROW}>
      <span data-voice-case="0">
        <VoiceWave activity="listening" level={0.5} size="md" label="Voice">
          <span data-voice-child="" style={{ ...DOT, width: 24, height: 24 }} />
        </VoiceWave>
      </span>
      <span data-voice-case="1">
        <VoiceWave activity="speaking" level={0.3} size="lg">
          <span data-voice-child="" style={{ ...DOT, width: 24, height: 24 }} />
        </VoiceWave>
      </span>
      <span data-voice-case="2">
        <VoiceWave activity="thinking" size="xl">
          <span data-voice-child="" style={{ ...DOT, width: 120, height: 20 }} />
        </VoiceWave>
      </span>
      <span data-voice-case="3">
        <VoiceWave activity="idle" size="sm">
          <span data-voice-child="" style={{ ...DOT, width: 24, height: 24 }} />
        </VoiceWave>
      </span>
      <VoiceWave activity="speaking" level={0.4} bands={[0.7, 1, 0.5, 0.8]} size="lg" label="Voice" />
      <VoiceWave activity="muted" size="md">
        <span style={{ ...DOT, width: 24, height: 24 }} />
      </VoiceWave>
    </div>
  ),
  "plain/disclosure-header": () => (
    <div style={{ ...COLUMN, width: 320 }}>
      <DisclosureHeader open onToggle={noop} data-xui-target="">
        Read 3 files
      </DisclosureHeader>
      <DisclosureHeader open={false} onToggle={noop}>
        Subagent lane
      </DisclosureHeader>
      <DisclosureHeader open={false} onToggle={noop} chevron="trailing">
        <span>Output</span>
      </DisclosureHeader>
      <DisclosureHeader open={false} onToggle={noop} disabled>
        Disabled fold
      </DisclosureHeader>
    </div>
  ),
  "plain/choice-tiles": () => (
    <div style={{ ...COLUMN, width: 520 }}>
      <ChoiceTiles
        aria-label="Layout"
        value="two"
        onChange={noop}
        columns={3}
        options={[
          { value: `two`, label: `Two columns`, description: `Pictures beside the text`, image: PICTURE },
          { value: `one`, label: `One column` },
          { value: `off`, label: `Off`, disabled: true },
        ]}
      />
      <ChoiceTiles
        aria-label="Targets"
        multiple
        invalid
        value={[]}
        onChange={noop}
        options={[
          { value: `web`, label: `Web` },
          { value: `ios`, label: `iOS`, description: `Simulator` },
          { value: `android`, label: `Android` },
        ]}
      />
    </div>
  ),
  "plain/code-block": () => (
    <div style={{ ...COLUMN, width: 560 }}>
      <CodeBlock>
        <CodeBlockHeader>
          <CodeBlockTitle>lines.ts</CodeBlockTitle>
          <CodeBlockActions>
            <CodeBlockCopyButton value={SOURCE} />
          </CodeBlockActions>
        </CodeBlockHeader>
        <CodeBlockContent language="ts" lineNumbers>
          {SOURCE}
        </CodeBlockContent>
      </CodeBlock>
      <CodeBlock size="sm" variant="destructive">
        <CodeBlockContent language="ansi">{`> Task :app:compile\nerror: unresolved reference\nBUILD FAILED in 3s`}</CodeBlockContent>
      </CodeBlock>
      <CodeBlock>
        <CodeBlockContent>
          <CodeBlockLine variant="removed">const a = 1</CodeBlockLine>
          <CodeBlockLine variant="added">const a = 2</CodeBlockLine>
          <CodeBlockLine variant="highlighted">return a</CodeBlockLine>
          <CodeBlockLine />
        </CodeBlockContent>
        <CodeBlockFooter>3 lines</CodeBlockFooter>
      </CodeBlock>
    </div>
  ),
  "plain/table": () => (
    <div style={{ width: 560 }}>
      <Table>
        <TableCaption>Seats on the team</TableCaption>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead>Role</TableHead>
            <TableHead>Boards</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow>
            <TableCell>Ada Lovelace</TableCell>
            <TableCell>Owner</TableCell>
            <TableCell>4</TableCell>
          </TableRow>
          <TableRow data-state="selected">
            <TableCell>Grace Hopper</TableCell>
            <TableCell>Member</TableCell>
            <TableCell>2</TableCell>
          </TableRow>
          <TableRow>
            <TableCell>Alan Turing</TableCell>
            <TableCell>Member</TableCell>
            <TableCell>1</TableCell>
          </TableRow>
        </TableBody>
        <TableFooter>
          <TableRow>
            <TableCell colSpan={2}>Total</TableCell>
            <TableCell>7</TableCell>
          </TableRow>
        </TableFooter>
      </Table>
    </div>
  ),
  "plain/breadcrumb": () => (
    <Breadcrumb>
      <BreadcrumbList>
        <BreadcrumbItem>
          <BreadcrumbLink href="#home" data-xui-target="">
            Home
          </BreadcrumbLink>
        </BreadcrumbItem>
        <BreadcrumbSeparator />
        <BreadcrumbItem>
          <BreadcrumbEllipsis />
        </BreadcrumbItem>
        <BreadcrumbSeparator />
        <BreadcrumbItem>
          <BreadcrumbLink asChild>
            <button type="button">Boards</button>
          </BreadcrumbLink>
        </BreadcrumbItem>
        <BreadcrumbSeparator>/</BreadcrumbSeparator>
        <BreadcrumbItem>
          <BreadcrumbPage>Issue FEED-131</BreadcrumbPage>
        </BreadcrumbItem>
      </BreadcrumbList>
    </Breadcrumb>
  ),
  // N-12: THE DockCard (one opening, one with a trailing action, one hidden
  // behind the front) and THE pile (fixed heights: collapsed and fanned).
  "plain/dock-card": () => (
    <div style={{ display: `flex`, flexDirection: `column`, gap: 8, width: 420 }}>
      <DockCard tint="plan" icon={Sparkles} label="Plan ready" caption="Ask dock" meta="3 questions" title="Fold the composer into the Dock" onOpen={() => {}} openLabel="Plan ready: Fold the composer into the Dock" />
      <DockCard tint="warning" icon={CircleAlert} label="Usage limit" title="Claude's 5-hour window is used up" trailing={<button type="button">Switch account</button>} />
      <DockCard tint="muted" icon={Inbox} label="2 queued" title="also check the watchdog" onOpen={() => {}} hidden />
    </div>
  ),
  "plain/dock-pile": () => (
    <div style={{ display: `flex`, flexDirection: `column`, gap: 24, width: 420 }}>
      {[false, true].map((expanded) => (
        <DockPile
          key={String(expanded)}
          expanded={expanded}
          items={[`2 queued`, `3 to do`, `Question`, `Plan ready`].map((label, i, all) => ({
            id: label,
            height: 62,
            node: <DockCard tint={i === 3 ? `plan` : `muted`} icon={i === 3 ? Sparkles : Inbox} label={label} title={`Card ${i + 1}`} onOpen={() => {}} hidden={!expanded && i < all.length - 1} />,
          }))}
        />
      ))}
    </div>
  ),
}
