import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@exp/ui"
import { upstreamPicker, type UpstreamPickerRow } from "@exp/domain-contract/apps"

// #VAPP-254 (apps.json `upstreamPickerCases`): which team server fills an
// upstream the app declares: the Accounts row's and the import step's Select.
export function UpstreamPickerSelect({
  row,
  servers,
  disabled,
  testId,
  onPick,
}: {
  row: UpstreamPickerRow
  servers: readonly { id: string; name: string }[]
  disabled?: boolean
  testId: string
  onPick: (value: string) => void
}) {
  const picker = upstreamPicker(row, servers)
  if (!picker) return null
  return (
    <Select value={picker.value} disabled={disabled} onValueChange={onPick}>
      <SelectTrigger size="sm" className="w-36 shrink-0" aria-label={row.name} data-testid={testId}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {picker.options.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
