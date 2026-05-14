import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'

const NONE_VALUE = '__none__'

export interface ProjectOption {
  path: string
  title: string
}

export interface ProjectCellProps {
  value: string | null
  options: ProjectOption[]
  onChange: (path: string | null) => void
  placeholder?: string
  emptyLabel?: string
  noProjectsLabel?: string
  disabled?: boolean
}

export function ProjectCell({
  value,
  options,
  onChange,
  placeholder = 'Project',
  emptyLabel = 'None',
  noProjectsLabel = 'No projects in this vault',
  disabled = false,
}: ProjectCellProps) {
  const handleChange = (next: string) => {
    onChange(next === NONE_VALUE ? null : next)
  }

  return (
    <Select value={value ?? NONE_VALUE} onValueChange={handleChange} disabled={disabled}>
      <SelectTrigger
        size="sm"
        className="h-8 w-40 border-border bg-[var(--bg-input)] px-2 text-xs"
        data-testid="task-project-select"
      >
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent align="start" className="z-[13000]">
        <SelectItem value={NONE_VALUE} className="text-xs">
          {emptyLabel}
        </SelectItem>
        {options.length === 0 ? (
          <div className="px-2 py-1 text-xs text-muted-foreground">{noProjectsLabel}</div>
        ) : (
          options.map((option) => (
            <SelectItem key={option.path} value={option.path} className="text-xs">
              {option.title}
            </SelectItem>
          ))
        )}
      </SelectContent>
    </Select>
  )
}
