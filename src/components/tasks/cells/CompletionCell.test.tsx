import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { CompletionCell } from './CompletionCell'

describe('CompletionCell', () => {
  it('renders the current value', () => {
    render(<CompletionCell value={42} onChange={vi.fn()} />)
    const input = screen.getByTestId('task-completion-input') as HTMLInputElement
    expect(input.value).toBe('42')
  })

  it('emits a number on blur', () => {
    const onChange = vi.fn()
    render(<CompletionCell value={null} onChange={onChange} />)
    const input = screen.getByTestId('task-completion-input')
    fireEvent.change(input, { target: { value: '60' } })
    fireEvent.blur(input)
    expect(onChange).toHaveBeenCalledWith(60)
  })

  it('clamps values above 100 to 100', () => {
    const onChange = vi.fn()
    render(<CompletionCell value={null} onChange={onChange} />)
    const input = screen.getByTestId('task-completion-input')
    fireEvent.change(input, { target: { value: '150' } })
    fireEvent.blur(input)
    expect(onChange).toHaveBeenCalledWith(100)
  })

  it('clamps negative values to 0', () => {
    const onChange = vi.fn()
    render(<CompletionCell value={null} onChange={onChange} />)
    const input = screen.getByTestId('task-completion-input')
    fireEvent.change(input, { target: { value: '-5' } })
    fireEvent.blur(input)
    expect(onChange).toHaveBeenCalledWith(0)
  })

  it('emits null when cleared', () => {
    const onChange = vi.fn()
    render(<CompletionCell value={50} onChange={onChange} />)
    const input = screen.getByTestId('task-completion-input')
    fireEvent.change(input, { target: { value: '' } })
    fireEvent.blur(input)
    expect(onChange).toHaveBeenCalledWith(null)
  })

})
