import { Settings2 } from 'lucide-react'

import type { EdgeDisplaySettings } from '@/shared/graph'
import { Button } from '@/shared/ui/button'
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/shared/ui/dropdown-menu'

type GraphEdgeSettingsProps = {
  value: EdgeDisplaySettings
  onChange: (patch: Partial<EdgeDisplaySettings>) => void
}

const OPTIONS: { key: keyof EdgeDisplaySettings; label: string; hint: string }[] = [
  { key: 'showLabels', label: 'Amount & currency labels', hint: 'Draw text on each edge' },
  { key: 'colorByCurrency', label: 'Colour by currency', hint: 'Tint edges per asset' },
  { key: 'widthByAmount', label: 'Width by amount', hint: 'Thicker = larger transfer' },
  { key: 'aggregate', label: 'Merge & count transfers', hint: 'One arc per pair with a count' },
]

/** Toolbar control that toggles how transfer edges are drawn. */
export function GraphEdgeSettings({ value, onChange }: GraphEdgeSettingsProps) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size='sm' variant='outline' className='h-8'>
          <Settings2 />
          Edges
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align='end' className='w-64'>
        <DropdownMenuLabel>Transfer edges</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {OPTIONS.map((option) => (
          <DropdownMenuCheckboxItem
            key={option.key}
            checked={value[option.key]}
            onCheckedChange={(checked) => onChange({ [option.key]: checked === true })}
            onSelect={(event) => event.preventDefault()}
          >
            <div className='flex flex-col'>
              <span>{option.label}</span>
              <span className='text-xs text-muted-foreground'>{option.hint}</span>
            </div>
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
