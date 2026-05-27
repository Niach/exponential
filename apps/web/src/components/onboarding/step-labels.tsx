import { useState } from "react"
import { Tag, Plus } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { LABEL_COLORS } from "@/lib/label-colors"
import { ColorSwatchGrid } from "@/components/ui/color-swatch-grid"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Badge } from "@/components/ui/badge"
import type { StepProps } from "./wizard"

export function StepLabels({ workspaceId, onNext }: StepProps) {
  const [name, setName] = useState(``)
  const [color, setColor] = useState(LABEL_COLORS[0])
  const [created, setCreated] = useState<{ name: string; color: string }[]>([])
  const [saving, setSaving] = useState(false)

  const handleAdd = async () => {
    if (!name.trim()) return
    setSaving(true)
    try {
      await trpc.labels.create.mutate({
        workspaceId,
        name: name.trim(),
        color,
      })
      setCreated((prev) => [...prev, { name: name.trim(), color }])
      setName(``)
      setColor(LABEL_COLORS[(created.length + 1) % LABEL_COLORS.length])
    } finally {
      setSaving(false)
    }
  }

  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <Tag className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Create labels</CardTitle>
        <CardDescription>
          Labels help you categorize and filter issues. Common examples: bug,
          feature, improvement.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {created.length > 0 && (
          <div className="flex flex-wrap gap-2">
            {created.map((label) => (
              <Badge
                key={label.name}
                variant="outline"
                className="gap-1.5"
              >
                <div
                  className="h-2.5 w-2.5 rounded-full"
                  style={{ backgroundColor: label.color }}
                />
                {label.name}
              </Badge>
            ))}
          </div>
        )}

        <div className="space-y-3">
          <div className="space-y-2">
            <Label htmlFor="onb-label-name">Label name</Label>
            <div className="flex gap-2">
              <Input
                id="onb-label-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. bug"
                autoFocus
                onKeyDown={(e) => {
                  if (e.key === `Enter`) {
                    e.preventDefault()
                    void handleAdd()
                  }
                }}
              />
              <Button
                size="icon"
                variant="outline"
                onClick={() => void handleAdd()}
                disabled={!name.trim() || saving}
              >
                <Plus className="size-4" />
              </Button>
            </div>
          </div>
          <ColorSwatchGrid value={color} onChange={setColor} />
        </div>

        <div className="flex justify-end">
          <Button onClick={onNext}>
            {created.length > 0 ? `Continue` : `Skip`}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
