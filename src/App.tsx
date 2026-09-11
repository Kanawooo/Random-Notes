import React, { useState, useEffect, useCallback, useRef, useMemo } from 'react'
import { suijian } from './lib/api'
import { SearchBar } from './components/SearchBar'
import { NoteList } from './components/NoteList'
import { Editor } from './components/Editor'
import { BatchActionBar } from './components/BatchActionBar'
import { SettingsView } from './components/SettingsView'
import { TagModal } from './components/TagModal'
import { ConfirmDialog } from './components/ConfirmDialog'
import { UiIcon } from './components/UiIcon'
import type { Note, NoteScope, Tag, AppSettings, RecoveryStatus } from './types'
import { toErrMsg } from './lib/errors'
import { normalizeShortcutSetting } from './lib/shortcuts'

// 键盘事件归一化（纯函数，模块顶层定义避免每次渲染重建闭包；配置值侧归一化见 lib/shortcuts）
function normalizeCombo(e: KeyboardEvent): string {
  const parts: string[] = []
  if (e.ctrlKey) parts.push('CTRL')
  if (e.altKey) parts.push('ALT')
  if (e.shiftKey) parts.push('SHIFT')
  if (e.metaKey) parts.push('SUPER')

  let key = e.key.toUpperCase()
  if (['CONTROL', 'ALT', 'SHIFT', 'META'].includes(key)) {
    return ''
  }
  if (key === ' ' || key === 'SPACEBAR') {
    key = 'SPACE'
  } else if (key === 'ESC') {
    key = 'ESCAPE'
  }
  parts.push(key)
  return parts.join('+')
}

export const App: React.FC = () => {
  const [view, setView] = useState<'search' | 'editor' | 'settings'>('search')
  const [currentNote, setCurrentNote] = useState<Note | null>(null)
  const [notes, setNotes] = useState<Note[]>([])
  const [tags, setTags] = useState<Tag[]>([])
  const [scope, setScope] = useState<NoteScope>('active')
  const [searchQuery, setSearchQuery] = useState('')
  const [selectedTagId, setSelectedTagId] = useState<string | null>(null)
  const [focusedIndex, setFocusedIndex] = useState(0)
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [focusTrigger, setFocusTrigger] = useState(0)

  const [isTagModalOpen, setIsTagModalOpen] = useState(false)
  const [settings, setSettings] = useState<AppSettings | null>(null)
  const [hotkeyWarning, setHotkeyWarning] = useState<string | null>(null)
  const [recoveryStatus, setRecoveryStatus] = useState<RecoveryStatus | null>(null)
  const [isInitialized, setIsInitialized] = useState(false)
  const [errorBanner, setErrorBanner] = useState<string | null>(null)
  const [successBanner, setSuccessBanner] = useState<string | null>(null)
  const [appVersion, setAppVersion] = useState('')

  const registeredFlushRef = useRef<(() => Promise<boolean>) | null>(null)

  const [confirmState, setConfirmState] = useState<{
    isOpen: boolean
    title: string
    message: string
    isDanger?: boolean
    onConfirm: () => void
  }>({
    isOpen: false,
    title: '',
    message: '',
    onConfirm: () => {}
  })

  // Refresh notes list
  // 请求序号守卫：快速连续刷新时，陈旧响应不得覆盖新结果集，也不得用旧列表裁剪已选 id
  const refreshSeqRef = useRef(0)
  const refreshNotes = useCallback(async () => {
    if (recoveryStatus?.isRecovery) {
      return
    }
    const seq = ++refreshSeqRef.current
    try {
      let list: Note[]
      if (searchQuery.trim()) {
        list = await suijian.notes.search(searchQuery.trim(), scope)
      } else {
        list = await suijian.notes.list(scope)
      }
      if (selectedTagId) {
        list = list.filter((n) => n.tags?.some((t) => t.id === selectedTagId))
      }
      if (seq !== refreshSeqRef.current) return
      setNotes(list)
      setFocusedIndex(0)
      setSelectedIds((prev) => {
        const next = new Set<string>()
        for (const id of prev) {
          if (list.some((n) => n.id === id)) {
            next.add(id)
          }
        }
        return next
      })
    } catch (err) {
      if (seq !== refreshSeqRef.current) return // 陈旧失败不得在新请求成功后弹横幅
      console.error('Failed to load notes:', err)
      setErrorBanner('加载便签列表失败：' + toErrMsg(err))
    }
  }, [searchQuery, scope, selectedTagId, recoveryStatus?.isRecovery])

  const refreshTags = useCallback(async () => {
    if (recoveryStatus?.isRecovery) {
      return
    }
    try {
      const list = await suijian.tags.list()
      setTags(list)
      // 筛选标签可能已被其他入口（设置页/便签内标签弹窗）删除，不在最新列表则清空筛选，避免列表空且无法解除
      setSelectedTagId((prev) => (prev && !list.some((t) => t.id === prev) ? null : prev))
    } catch (err) {
      console.error('Failed to load tags:', err)
      setErrorBanner('加载标签失败：' + toErrMsg(err))
    }
  }, [recoveryStatus?.isRecovery])

  // Safe navigation back to search
  const handleBackToSearch = useCallback(async () => {
    if (registeredFlushRef.current) {
      const ok = await registeredFlushRef.current()
      if (!ok) {
        setErrorBanner('便签正在保存中或保存失败，无法返回列表')
        return false
      }
    }
    setView('search')
    setFocusTrigger((p) => p + 1)
    refreshNotes()
    return true
  }, [refreshNotes])

  // Create empty draft or reuse existing
  const handleCreateNote = useCallback(
    async (reuseDraft = true, titleOverride?: string) => {
      if (recoveryStatus?.isRecovery) {
        setErrorBanner('当前处于数据只读保护模式，无法新建便签')
        return
      }
      if (registeredFlushRef.current) {
        const ok = await registeredFlushRef.current()
        if (!ok) {
          setErrorBanner('便签正在保存中或保存失败，无法新建便签')
          return
        }
      }
      try {
        const newNote = await suijian.notes.create({
          title: titleOverride,
          reuse_empty_draft: reuseDraft
        })
        setCurrentNote(newNote)
        setView('editor')
        refreshNotes()
      } catch (err) {
        console.error('Failed to create note:', err)
        setErrorBanner('新建便签失败：' + toErrMsg(err))
      }
    },
    [recoveryStatus?.isRecovery, refreshNotes]
  )

  // Safe navigation to settings (flushing unsaved editor content first)
  const handleOpenSettings = useCallback(async () => {
    if (registeredFlushRef.current) {
      const ok = await registeredFlushRef.current()
      if (!ok) {
        setErrorBanner('便签正在保存中或保存失败，无法进入设置，请重试')
        return
      }
    }
    setView('settings')
  }, [])

  // 供一次性注册的 Tauri 事件回调读取最新视图/刷新函数，避免捕获首帧陈旧闭包
  const viewRef = useRef(view)
  viewRef.current = view

  const refreshNotesRef = useRef(refreshNotes)
  refreshNotesRef.current = refreshNotes

  const handleCreateNoteRef = useRef(handleCreateNote)
  handleCreateNoteRef.current = handleCreateNote

  const handleTagsChangedInSettings = useCallback(
    (deletedTagId?: string) => {
      if (deletedTagId && selectedTagId === deletedTagId) {
        setSelectedTagId(null)
      }
      refreshTags()
      refreshNotes()
    },
    [selectedTagId, refreshTags, refreshNotes]
  )

  // App initialization and Tauri IPC events orchestration
  useEffect(() => {
    let isMounted = true
    let unlistenFns: (() => void)[] = []

    const bootstrap = async () => {
      try {
        const status = await suijian.app.getRecoveryStatus()
        if (!isMounted) return
        setRecoveryStatus(status)
        setIsInitialized(true)

        // 版本号仅用于底栏展示，读取失败不阻塞初始化
        try {
          const info = await suijian.app.getInfo()
          if (isMounted) {
            setAppVersion(info.version)
          }
        } catch (err) {
          console.error('Failed to get app info:', err)
        }

        if (!status.isRecovery) {
          const [s, t] = await Promise.all([
            suijian.settings.getAll(),
            suijian.tags.list()
          ])
          if (!isMounted) return
          setSettings(s)
          setTags(t)
        }

        try {
          const hk = await suijian.settings.getHotkeyStatus()
          if (isMounted && !hk.registered) {
            setHotkeyWarning(
              `全局呼出快捷键 (${hk.currentHotkey || hk.requestedHotkey || '快捷键'}) 注册失败，可能已被其他软件占用。建议在设置中修改为 ${hk.recommendedHotkey || 'Ctrl+Shift+Space'}`
            )
          }
        } catch (err) {
          console.error('Failed to get hotkey status:', err)
        }

        // Register all Tauri IPC events and await registration promises
        const [unFocus, unReqNew, unHide, unQuit] = await Promise.all([
          suijian.events.onFocusSearch(() => {
            // 保持隐藏前的视图；仅在搜索页时聚焦搜索框并刷新列表
            if (viewRef.current === 'search') {
              setFocusTrigger((p) => p + 1)
              refreshNotesRef.current()
            }
          }),
          suijian.events.onRequestNewNote(() => {
            handleCreateNoteRef.current(true)
          }),
          suijian.events.onRequestHide(async () => {
            try {
              if (registeredFlushRef.current) {
                const ok = await registeredFlushRef.current()
                if (!ok) return
              }
              await suijian.window.confirmHide()
            } catch (err) {
              console.error('隐藏窗口失败:', err)
            }
          }),
          suijian.events.onRequestQuit(async () => {
            try {
              if (registeredFlushRef.current) {
                const ok = await registeredFlushRef.current()
                if (!ok) {
                  setErrorBanner('有内容尚未保存成功，已取消退出；请重试保存后再退出')
                  return
                }
              }
              await suijian.app.confirmQuit()
            } catch (err) {
              console.error('退出流程失败:', err)
            }
          })
        ])

        if (!isMounted) {
          unFocus()
          unReqNew()
          unHide()
          unQuit()
          return
        }

        unlistenFns = [unFocus, unReqNew, unHide, unQuit]

        // Notify backend renderer ready only after initialization and listeners are all active
        try {
          await suijian.app.notifyRendererReady()
        } catch (err) {
          console.error('Failed to notify backend renderer_ready:', err)
        }
      } catch (err) {
        console.error('App bootstrap error:', err)
        if (isMounted) {
          setIsInitialized(true)
          setErrorBanner('应用初始化失败：' + toErrMsg(err))
        }
      }
    }

    bootstrap()

    return () => {
      isMounted = false
      for (const fn of unlistenFns) {
        try {
          fn()
        } catch { /* unlisten 清理失败可忽略 */ }
      }
    }
  }, [])

  // scope/标签/初始化/恢复模式变化即时刷新；searchQuery 由下方防抖 effect 处理。
  // 守卫与 deps 必须完整：恢复模式不得发 notes.list（app-workflow.test.tsx:296 护栏）
  useEffect(() => {
    if (isInitialized && !recoveryStatus?.isRecovery) {
      refreshNotesRef.current()
    }
  }, [isInitialized, recoveryStatus?.isRecovery, scope, selectedTagId])

  // 搜索防抖：每按键不再直发 IPC+SQLite 查询
  useEffect(() => {
    if (!isInitialized || recoveryStatus?.isRecovery) return
    const t = setTimeout(() => refreshNotesRef.current(), 250)
    return () => clearTimeout(t)
  }, [searchQuery, isInitialized, recoveryStatus?.isRecovery])

  // 恢复成功横幅 5 秒后自动消失（再次触发时计时重置）
  useEffect(() => {
    if (!successBanner) return
    const t = setTimeout(() => setSuccessBanner(null), 5000)
    return () => clearTimeout(t)
  }, [successBanner])

  // Normalization helpers for keyboard events
  // 两个可配置快捷键的归一化结果按设置缓存，避免每次按键处理重复解析（收起统一由全局召唤键承担）
  const shortcutCombos = useMemo(
    () => ({
      newNote: normalizeShortcutSetting(settings?.shortcutNewNote || 'Ctrl+N'),
      back: normalizeShortcutSetting(settings?.shortcutBackToSearch || 'Ctrl+E'),
    }),
    [settings?.shortcutNewNote, settings?.shortcutBackToSearch]
  )

  // 应用级保留快捷键：编辑器正文中命中时必须放行给 window 全局处理器（Editor 的 handleDOMEvents 消费）。
  // ESCAPE 固定放行：正文内 Escape=返回搜索是产品保留职责，而 ProseMirror captureKeyDown 对 keyCode 27
  // 无条件 preventDefault（prosemirror-view capturekeys.ts），不放行则被 App 的 defaultPrevented 早退吞掉。
  // IME 安全由 isAppReservedShortcut 裸键路径的 isComposing/229 短路守卫保障：组合中 Escape 交还输入法，不误返回
  const reservedShortcuts = useMemo(() => {
    const set = new Set<string>(['ESCAPE'])
    if (shortcutCombos.back) set.add(shortcutCombos.back)
    if (shortcutCombos.newNote) set.add(shortcutCombos.newNote)
    return set
  }, [shortcutCombos])

  const isAppReservedShortcut = useCallback(
    (e: KeyboardEvent): boolean => {
      const combo = normalizeCombo(e)
      if (combo === '') return false
      // 带修饰键的功能组合不受 sticky isComposing 吞并（与 handleKeyDown 的 imeBlocked 收紧同源）。
      // IME 组合中真实被拦截的键下发 Process/229，归一化为 'CTRL+PROCESS'，
      // 不可能命中 reservedShortcuts（成员全部来自后端校验过的归一化串），天然交还编辑器
      if (e.ctrlKey || e.altKey || e.metaKey) return reservedShortcuts.has(combo)
      if (e.isComposing || e.keyCode === 229) return false // 裸键：IME 进行中绝不拦截
      return reservedShortcuts.has(combo)
    },
    [reservedShortcuts]
  )

  // Global Keyboard Handlers
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.defaultPrevented) {
        return
      }
      // IME 守卫收紧（双向审查 H1 裁定）：229/Process 一律交还输入法；
      // sticky isComposing（组合已结束但标志未清，WebView2/部分 IME 真实病理态）
      // 不再吞带 Ctrl/Alt/Super 的功能键组合——“配置的键就该管用”（用户拍板），
      // 裸键无修饰仍全部让位 IME（候选翻页 Shift/Enter 等路径与收紧前逐键相同）
      const imeBlocked = e.keyCode === 229 || (e.isComposing && !(e.ctrlKey || e.altKey || e.metaKey))
      if (imeBlocked) {
        return
      }

      const { newNote: shortcutNewNote, back: shortcutBack } = shortcutCombos

      const currentCombo = normalizeCombo(e)

      // 1. Bare Escape handling
      if (e.key === 'Escape') {
        if (confirmState.isOpen) {
          e.preventDefault()
          setConfirmState((prev) => ({ ...prev, isOpen: false }))
          return
        }
        if (isTagModalOpen) {
          e.preventDefault()
          setIsTagModalOpen(false)
          return
        }
        if (view === 'settings' || view === 'editor') {
          e.preventDefault()
          handleBackToSearch()
          return
        }
        if (view === 'search') {
          if (searchQuery.trim().length > 0) {
            e.preventDefault()
            setSearchQuery('')
            return
          }
        }
      }

      // 2. Return to search shortcut (strictly match configured shortcut)
      if (currentCombo === shortcutBack) {
        e.preventDefault()
        if (view !== 'search') {
          handleBackToSearch()
        }
        return
      }

      // 3. New Note shortcut (strictly match configured shortcut)
      if (currentCombo === shortcutNewNote) {
        e.preventDefault()
        handleCreateNote(true)
        return
      }

      // 4. Arrow Up / Down / Enter / Space in Search view
      if (view === 'search' && !isTagModalOpen && !confirmState.isOpen) {
        const target = e.target as HTMLElement
        // 按钮聚焦时 Enter/Space 是原生激活语义，交还浏览器默认行为；方向键仍执行列表导航
        const onButton = target instanceof Element && target.closest('button') !== null
        const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA'

        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setFocusedIndex((prev) => (prev < notes.length - 1 ? prev + 1 : prev))
        } else if (e.key === 'ArrowUp') {
          e.preventDefault()
          setFocusedIndex((prev) => (prev > 0 ? prev - 1 : 0))
        } else if (e.key === 'Enter') {
          const isCheckbox = target instanceof HTMLInputElement && target.type === 'checkbox'
          if (isCheckbox) return
          if (onButton) return
          if (notes.length > 0 && focusedIndex >= 0 && focusedIndex < notes.length) {
            e.preventDefault()
            setCurrentNote(notes[focusedIndex])
            setView('editor')
          } else if (searchQuery.trim() && isInput) {
            e.preventDefault()
            handleCreateNote(false, searchQuery.trim())
          }
        } else if (e.key === ' ' && !isInput) {
          if (onButton) return
          e.preventDefault()
          if (notes[focusedIndex]) {
            const id = notes[focusedIndex].id
            setSelectedIds((prev) => {
              const next = new Set(prev)
              if (next.has(id)) next.delete(id)
              else next.add(id)
              return next
            })
          }
        }
      }
    },
    [
      shortcutCombos,
      view,
      confirmState.isOpen,
      isTagModalOpen,
      notes,
      focusedIndex,
      searchQuery,
      handleCreateNote,
      handleBackToSearch
    ]
  )

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [handleKeyDown])

  // Batch actions
  // 以下回调喂给 memo 化的 NoteList/列表项，引用必须稳定；内联箭头函数会使整表 memo 失效
  const handleToggleCheck = useCallback((id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }, [])

  const handleSelectNote = useCallback((note: Note) => {
    setCurrentNote(note)
    setView('editor')
  }, [])

  const handleFocusIndex = useCallback((index: number) => {
    setFocusedIndex(index)
  }, [])

  const handleToggleSelectAll = () => {
    if (selectedIds.size === notes.length && notes.length > 0) {
      setSelectedIds(new Set())
    } else {
      setSelectedIds(new Set(notes.map((n) => n.id)))
    }
  }

  const handleBatchTrash = () => {
    const ids = Array.from(selectedIds)
    setConfirmState({
      isOpen: true,
      title: '移入回收站',
      message: `确定要将选中的 ${ids.length} 篇便签移入回收站吗？`,
      isDanger: true,
      onConfirm: async () => {
        setConfirmState((p) => ({ ...p, isOpen: false }))
        try {
          await suijian.notes.trashMany(ids)
          setSelectedIds(new Set())
          refreshNotes()
        } catch (err) {
          console.error(err)
          setErrorBanner('批量移入回收站失败：' + toErrMsg(err))
        }
      }
    })
  }

  const handleBatchDeletePermanently = () => {
    const ids = Array.from(selectedIds)
    setConfirmState({
      isOpen: true,
      title: '彻底删除便签',
      message: `此操作将永久删除选中的 ${ids.length} 篇便签及其附件，不可恢复。确定继续吗？`,
      isDanger: true,
      onConfirm: async () => {
        setConfirmState((p) => ({ ...p, isOpen: false }))
        try {
          await suijian.notes.deletePermanentlyMany(ids)
          setSelectedIds(new Set())
          refreshNotes()
        } catch (err) {
          console.error(err)
          setErrorBanner('彻底删除失败：' + toErrMsg(err))
        }
      }
    })
  }

  const handleEmptyTrash = () => {
    setConfirmState({
      isOpen: true,
      title: '清空回收站',
      message: '确定要清空回收站中的全部便签和附件吗？此操作无法撤销。',
      isDanger: true,
      onConfirm: async () => {
        setConfirmState((p) => ({ ...p, isOpen: false }))
        try {
          await suijian.notes.emptyTrash()
          setSelectedIds(new Set())
          refreshNotes()
        } catch (err) {
          console.error(err)
          setErrorBanner('清空回收站失败：' + toErrMsg(err))
        }
      }
    })
  }

  return (
    <div className="app-container">
      {/* 顶部标题栏 / 快捷导航 */}
      <div className="top-bar" data-tauri-drag-region>
        <div className="brand-badge" data-tauri-drag-region>
          <div className="brand-dot" />
          <span>随笺</span>
        </div>

        <div className="top-actions no-drag">
          {view === 'search' && (
            <>
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => handleCreateNote(true)}
                aria-label="+ 新建"
                title={`新建便签 (${settings?.shortcutNewNote || 'Ctrl+N'})`}
              >
                + 新建
              </button>
              <button
                type="button"
                className="btn"
                onClick={handleOpenSettings}
                aria-label="设置"
                title="设置"
              >
                <UiIcon name="settings" size={14} style={{ marginRight: 4 }} />
                设置
              </button>
            </>
          )}

          {view === 'editor' && (
            <button
              type="button"
              className="btn"
              onClick={handleBackToSearch}
              aria-label="返回搜索"
              title={`返回搜索 (${settings?.shortcutBackToSearch || 'Ctrl+E'})`}
            >
              ← 搜索 ({settings?.shortcutBackToSearch || 'Ctrl+E'})
            </button>
          )}

          {view === 'settings' && (
            <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
              <button
                type="button"
                className="btn"
                onClick={handleBackToSearch}
                aria-label="返回搜索"
                title={`返回搜索 (${settings?.shortcutBackToSearch || 'Ctrl+E'})`}
              >
                ← 搜索 ({settings?.shortcutBackToSearch || 'Ctrl+E'})
              </button>
              <button
                type="button"
                className="btn btn-icon"
                onClick={handleBackToSearch}
                aria-label="关闭"
                title="关闭"
              >
                <UiIcon name="close" size={14} />
              </button>
            </div>
          )}
        </div>
      </div>

      {/* 错误提示横条 */}
      {errorBanner && (
        <div className="error-banner" role="alert">
          <span>{errorBanner}</span>
          <button
            type="button"
            className="btn btn-icon"
            onClick={() => setErrorBanner(null)}
            aria-label="关闭错误"
          >
            <UiIcon name="close" size={14} />
          </button>
        </div>
      )}

      {/* 恢复成功提示横条：恢复完成后设置页立即切走，文案上浮到 App 层才可见 */}
      {successBanner && (
        <div className="success-banner" role="status">
          <span>{successBanner}</span>
          <button
            type="button"
            className="btn btn-icon"
            onClick={() => setSuccessBanner(null)}
            aria-label="关闭提示"
          >
            <UiIcon name="close" size={14} />
          </button>
        </div>
      )}

      {/* 快捷键冲突警告横条 */}
      {hotkeyWarning && (
        <div className="warning-banner" role="alert" aria-label="热键冲突提示">
          <UiIcon name="warning" size={14} style={{ marginRight: 6 }} />
          <span>{hotkeyWarning}</span>
          {view !== 'settings' && (
            <button
              type="button"
              className="btn"
              style={{ padding: '2px 8px', fontSize: '12px', marginLeft: 'auto' }}
              onClick={handleOpenSettings}
              aria-label="前往设置热键"
            >
              设置
            </button>
          )}
          <button
            type="button"
            className="btn btn-icon"
            onClick={() => setHotkeyWarning(null)}
            aria-label="关闭提示"
          >
            <UiIcon name="close" size={14} />
          </button>
        </div>
      )}

      {/* 数据只读保护横条 */}
      {recoveryStatus?.isRecovery && (
        <div className="warning-banner" role="alert">
          <UiIcon name="shield" size={14} style={{ marginRight: 6 }} />
          <span>
            数据只读保护模式：{recoveryStatus.error || '数据库格式不匹配或损坏'}
          </span>
          <button
            type="button"
            className="btn"
            style={{ padding: '2px 8px', fontSize: '12px', marginLeft: 'auto' }}
            onClick={() =>
              suijian.app.openUserDataFolder().catch((err) => {
                console.error(err)
                setErrorBanner('打开数据目录失败：' + toErrMsg(err))
              })
            }
          >
            打开数据目录
          </button>
        </div>
      )}

      {/* 视图一：搜索与索引卡列表 */}
      {view === 'search' && (
        <div className="search-view">
          <SearchBar
            query={searchQuery}
            onQueryChange={setSearchQuery}
            scope={scope}
            onScopeChange={(s) => {
              setScope(s)
              setSelectedIds(new Set())
            }}
            tags={tags}
            selectedTagId={selectedTagId}
            onSelectTag={setSelectedTagId}
            totalCount={notes.length}
            newNoteShortcut={settings?.shortcutNewNote || 'Ctrl+N'}
            focusTrigger={focusTrigger}
          />

          <BatchActionBar
            selectedCount={selectedIds.size}
            totalCount={notes.length}
            allSelected={selectedIds.size === notes.length && notes.length > 0}
            scope={scope}
            onToggleSelectAll={handleToggleSelectAll}
            onClearSelection={() => setSelectedIds(new Set())}
            onBatchTrash={handleBatchTrash}
            onBatchDeletePermanently={handleBatchDeletePermanently}
            onEmptyTrash={handleEmptyTrash}
          />

          <NoteList
            notes={notes}
            focusedIndex={focusedIndex}
            selectedIds={selectedIds}
            onSelectNote={handleSelectNote}
            onToggleCheck={handleToggleCheck}
            onFocusIndex={handleFocusIndex}
            searchQuery={searchQuery}
            newNoteShortcut={
              // bootstrap 无条件存 recoveryStatus 对象，故必须用 ?.isRecovery 判恢复态；
              // isInitialized 门控避免启动瞬间闪一帧新建提示
              isInitialized && scope === 'active' && !selectedTagId && !recoveryStatus?.isRecovery
                ? settings?.shortcutNewNote || 'Ctrl+N'
                : undefined
            }
          />
        </div>
      )}

      {/* 视图二：富文本编辑器 */}
      {view === 'editor' && currentNote && (
        <Editor
          key={currentNote.id}
          note={currentNote}
          tags={tags}
          onBack={handleBackToSearch}
          onNoteUpdated={(updated) => {
            setCurrentNote(updated)
          }}
          onOpenTagModal={() => setIsTagModalOpen(true)}
          onRegisterFlush={(flush) => {
            registeredFlushRef.current = flush
            return () => {
              registeredFlushRef.current = null
            }
          }}
          isAppReservedShortcut={isAppReservedShortcut}
        />
      )}

      {/* 视图三：设置页面 */}
      {view === 'settings' && (
        <SettingsView
          onSettingsChanged={(newSettings) => setSettings(newSettings)}
          onTagsChanged={handleTagsChangedInSettings}
          onBeforeRestore={async () => {
            if (registeredFlushRef.current) {
              const ok = await registeredFlushRef.current()
              if (!ok) {
                setErrorBanner('当前便签正在保存中或保存失败，已阻止恢复操作以保护未保存内容')
                return false
              }
            }
            return true
          }}
          onDataRestored={(message) => {
            setCurrentNote(null)
            setView('search')
            setSelectedIds(new Set())
            setSelectedTagId(null)
            setSearchQuery('')
            setFocusedIndex(0)
            setFocusTrigger((p) => p + 1)
            setSuccessBanner(message)

            suijian.app.getRecoveryStatus().then((status) => {
              setRecoveryStatus(status.isRecovery ? status : null)
              if (!status.isRecovery) {
                refreshNotes()
                refreshTags()
              }
            }).catch((err) => {
              console.error(err)
              setErrorBanner('获取数据状态失败：' + toErrMsg(err))
            })
          }}
        />
      )}

      {/* 标签管理弹窗 */}
      <TagModal
        isOpen={isTagModalOpen}
        tags={tags}
        onClose={() => setIsTagModalOpen(false)}
        onRefreshTags={() => {
          refreshTags()
          refreshNotes()
        }}
      />

      {/* 确认对话框 */}
      <ConfirmDialog
        isOpen={confirmState.isOpen}
        title={confirmState.title}
        message={confirmState.message}
        isDanger={confirmState.isDanger}
        onConfirm={confirmState.onConfirm}
        onCancel={() => setConfirmState((p) => ({ ...p, isOpen: false }))}
      />

      {/* 底部快捷键提示 */}
      <div className="bottom-bar">
        <span>
          <span className="kbd-shortcut">{settings?.shortcutNewNote || 'Ctrl+N'}</span> 新建 &nbsp;|&nbsp;{' '}
          <span className="kbd-shortcut">{settings?.shortcutBackToSearch || 'Ctrl+E'}</span> 搜索 &nbsp;|&nbsp;{' '}
          <span className="kbd-shortcut">{settings?.hotkey || 'Ctrl+Space'}</span> 呼出/收起
        </span>
        <span>随笺 v{appVersion}</span>
      </div>
    </div>
  )
}
