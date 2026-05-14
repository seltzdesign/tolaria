import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ProjectCell, type ProjectOption } from './ProjectCell'

const options: ProjectOption[] = [
  { path: '/vault/alpha.md', title: 'Alpha' },
  { path: '/vault/beta.md', title: 'Beta' },
]

describe('ProjectCell', () => {
  it('shows the empty label when no project is selected', () => {
    render(
      <ProjectCell value={null} options={options} onChange={vi.fn()} placeholder="Project" emptyLabel="None" />,
    )
    expect(screen.getByTestId('task-project-select')).toHaveTextContent('None')
  })

  it('renders the selected option title when a value is provided', () => {
    render(<ProjectCell value={options[1].path} options={options} onChange={vi.fn()} />)
    expect(screen.getByTestId('task-project-select')).toHaveTextContent('Beta')
  })

  it('still renders the trigger when value does not match any option', () => {
    render(<ProjectCell value="/vault/missing.md" options={options} onChange={vi.fn()} placeholder="Project" />)
    expect(screen.getByTestId('task-project-select')).toBeInTheDocument()
  })
})
