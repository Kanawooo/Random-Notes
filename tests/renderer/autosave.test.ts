import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { useAutoSave } from '../../src/hooks/useAutoSave'
import { suijian } from '../../src/lib/api'
import type { Note } from '../../src/types'

vi.mock('../../src/lib/api', () => ({
  suijian: {
    notes: {
      update: vi.fn()
    }
  }
}))

describe('useAutoSave Hook', () => {
  let mockNote: Note
  let onNoteUpdated: ReturnType<typeof vi.fn>
  let onError: ReturnType<typeof vi.fn>

  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()

    mockNote = {
      id: 'test-note-uuid',
      title: '初始标题',
      content_json: '{}',
      plain_text: '',
      title_manually_edited: false,
      is_pinned: false,
      archived_at: null,
      deleted_at: null,
      revision: 0,
      created_at: '2026-01-01T00:00:00.000Z',
      updated_at: '2026-01-01T00:00:00.000Z'
    }

    onNoteUpdated = vi.fn()
    onError = vi.fn()
  })

  it('debounces auto-save by 300ms', async () => {
    const updatedNote: Note = { ...mockNote, title: '已保存标题', revision: 1 }
    vi.mocked(suijian.notes.update).mockResolvedValueOnce(updatedNote)

    const { result } = renderHook(() =>
      useAutoSave({
        note: mockNote,
        onNoteUpdated,
        onError
      })
    )

    act(() => {
      result.current.scheduleSave({ title: '已保存标题' })
    })

    expect(result.current.saveStatus).toBe('idle')
    expect(suijian.notes.update).not.toHaveBeenCalled()

    // Advance by 100ms: still debouncing
    act(() => {
      vi.advanceTimersByTime(100)
    })
    expect(suijian.notes.update).not.toHaveBeenCalled()

    // Advance past 300ms
    await act(async () => {
      vi.advanceTimersByTime(250)
    })

    expect(suijian.notes.update).toHaveBeenCalledTimes(1)
    expect(suijian.notes.update).toHaveBeenCalledWith(
      expect.objectContaining({
        id: 'test-note-uuid',
        title: '已保存标题',
        expectedRevision: 0
      })
    )
    expect(result.current.saveStatus).toBe('saved')
    expect(onNoteUpdated).toHaveBeenCalledWith(updatedNote)
  })

  it('flushSave flushes pending saves immediately and awaits completion', async () => {
    const updatedNote: Note = { ...mockNote, title: '立即刷盘', revision: 1 }
    vi.mocked(suijian.notes.update).mockResolvedValueOnce(updatedNote)

    const { result } = renderHook(() =>
      useAutoSave({
        note: mockNote,
        onNoteUpdated,
        onError
      })
    )

    act(() => {
      result.current.scheduleSave({ title: '立即刷盘' })
    })

    let flushResult: boolean | undefined
    await act(async () => {
      flushResult = await result.current.flushSave()
    })

    expect(flushResult).toBe(true)
    expect(suijian.notes.update).toHaveBeenCalledTimes(1)
    expect(result.current.saveStatus).toBe('saved')
  })

  it('marks status as error and supports retryLastSave upon save failure', async () => {
    vi.mocked(suijian.notes.update).mockRejectedValue(new Error('网络或DB锁定'))

    const { result } = renderHook(() =>
      useAutoSave({
        note: mockNote,
        onNoteUpdated,
        onError
      })
    )

    act(() => {
      result.current.scheduleSave({ title: '失败重试测试' })
    })

    await act(async () => {
      const ok = await result.current.flushSave()
      expect(ok).toBe(false)
    })

    expect(result.current.saveStatus).toBe('error')
    expect(onError).toHaveBeenCalledWith('网络或DB锁定')

    // Retry save
    const retrySuccessNote: Note = { ...mockNote, title: '失败重试测试', revision: 1 }
    vi.mocked(suijian.notes.update).mockResolvedValue(retrySuccessNote)

    await act(async () => {
      const retryOk = await result.current.retryLastSave()
      expect(retryOk).toBe(true)
    })

    expect(result.current.saveStatus).toBe('saved')
    expect(onNoteUpdated).toHaveBeenCalledWith(retrySuccessNote)
  })
})
