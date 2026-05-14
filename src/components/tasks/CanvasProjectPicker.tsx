import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { translate, type AppLocale } from '../../lib/i18n'
import type { CanvasProjectOption } from '../../lib/tasks/canvasProjectFilter'

const ALL_PROJECTS_VALUE = '__all__'

interface CanvasProjectPickerProps {
  projects: CanvasProjectOption[]
  value: string | null
  onChange: (path: string | null) => void
  locale: AppLocale
}

export function CanvasProjectPicker({ projects, value, onChange, locale }: CanvasProjectPickerProps) {
  const handleChange = (next: string) => {
    onChange(next === ALL_PROJECTS_VALUE ? null : next)
  }

  return (
    <label className="flex min-w-0 items-center gap-2 text-xs font-medium text-muted-foreground">
      <span className="shrink-0">{translate(locale, 'tasks.canvas.projectLabel')}</span>
      <Select value={value ?? ALL_PROJECTS_VALUE} onValueChange={handleChange}>
        <SelectTrigger
          size="sm"
          className="h-7 min-w-0 max-w-60 flex-1 border-border bg-[var(--bg-input)] px-2 text-xs"
          data-testid="canvas-project-picker"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent align="start" className="z-[13000]">
          <SelectItem value={ALL_PROJECTS_VALUE} className="text-xs">
            {translate(locale, 'tasks.canvas.allProjects')}
          </SelectItem>
          {projects.map((project) => (
            <SelectItem key={project.path} value={project.path} className="text-xs">
              {project.title}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </label>
  )
}
