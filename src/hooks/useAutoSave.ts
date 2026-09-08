import { useState, useRef, useEffect, useCallback } from 'react'
import { suijian } from '../lib/api'
import type { Note } from '../types'

// 防御式调用窗口命令：既有测试的 api mock 未提供 window 命名空间，失败不得打断保存流程
function notifyUnsavedError(hasError: boolean) {
  try {
    const win = suijian.window as unknown as
      | { setUnsavedError?: (v: boolean) => Promise<void> }
      | undefined
    void win?.setUnsavedError?.(hasError)?.catch(() => {})
  } catch {
    // ignore
  }
}

export type SaveStatus = 'idle' | 'saving' | 'saved' | 'error'

export interface PendingSaveData {
  title?: string
  content_json?: string
  plain_text?: string
  title_manually_edited?: boolean
  tag_ids?: string[]
}

interface UseAutoSaveOptions {
  note: Note | null
  onNoteUpdated: (updated: Note) => void
  onError?: (msg: string) => void
}

export function useAutoSave({ note, onNoteUpdated, onError }: UseAutoSaveOptions) {
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('idle')
  const noteRef = useRef(note)
  noteRef.current = note

  // Current tracking revision of the note
  const currentRevisionRef = useRef(note?.revision ?? 0)
  useEffect(() => {
    if (note) {
      currentRevisionRef.current = note.revision
    }
  }, [note])

  // 新编辑器会话起点：复位门控标志（旧便签的 pending 已在导航时 flush或被阻断）
  useEffect(() => {
    notifyUnsavedError(false)
  }, [note?.id])

  // Queue of unsaved changes
  const pendingDataRef = useRef<PendingSaveData | null>(null)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // In-flight execution promise
  const inFlightPromiseRef = useRef<Promise<Note> | null>(null)

  // Core save routine: runs serially until pendingDataRef is empty
  const processQueue = useCallback(async (): Promise<void> => {
    // If a save is already in-flight, wait for it first
    while (inFlightPromiseRef.current) {
      try {
        await inFlightPromiseRef.current
      } catch {
        // Error handling will be completed by the in-flight promise catch block
      }
    }

    if (!noteRef.current || !pendingDataRef.current) {
      return
    }

    const currentNoteId = noteRef.current.id
    const dataToSave = { ...pendingDataRef.current }
    pendingDataRef.current = null
    setSaveStatus('saving')

    const expectedRevision = currentRevisionRef.current

    const promise = suijian.notes.update({
      id: currentNoteId,
      expectedRevision,
      ...dataToSave
    })

    inFlightPromiseRef.current = promise

    try {
      const updated = await promise
      // Success: update revision tracking
      currentRevisionRef.current = updated.revision
      onNoteUpdated(updated)
      setSaveStatus('saved')
      notifyUnsavedError(false)
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err)
      // On error: MERGE the failed payload back so changes are never lost!
      pendingDataRef.current = {
        ...dataToSave,
        ...(pendingDataRef.current || {})
      }
      setSaveStatus('error')
      notifyUnsavedError(true)
      if (onError) onError(msg)
      throw err
    } finally {
      inFlightPromiseRef.current = null
    }

    // If more changes arrived while saving, continue processing queue
    if (pendingDataRef.current) {
      await processQueue()
    }
  }, [onNoteUpdated, onError])

  const scheduleSave = useCallback(
    (fields: PendingSaveData) => {
      pendingDataRef.current = {
        ...(pendingDataRef.current || {}),
        ...fields
      }
      setSaveStatus('idle')

      if (timerRef.current) {
        clearTimeout(timerRef.current)
      }

      timerRef.current = setTimeout(() => {
        timerRef.current = null
        processQueue().catch(() => {
          // Error captured in state
        })
      }, 300)
    },
    [processQueue]
  )

  const flushSave = useCallback(async (): Promise<boolean> => {
    if (timerRef.current) {
      clearTimeout(timerRef.current)
      timerRef.current = null
    }

    if (inFlightPromiseRef.current || pendingDataRef.current) {
      try {
        await processQueue()
        return true
      } catch {
        return false
      }
    }
    return true
  }, [processQueue])

  const retryLastSave = useCallback(async (): Promise<boolean> => {
    return flushSave()
  }, [flushSave])

  // 用户确认错误横幅后清门控标志；仍有未落盘数据时保持阻断，下次保存结果会重新置位
  const clearSaveError = useCallback(() => {
    if (!pendingDataRef.current && !inFlightPromiseRef.current) {
      notifyUnsavedError(false)
    }
  }, [])

  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current)
        timerRef.current = null
      }
    }
  }, [])

  return {
    saveStatus,
    scheduleSave,
    flushSave,
    retryLastSave,
    clearSaveError
  }
}

