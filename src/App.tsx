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
        const [unFocus, unReqNew, unCreated, unHide, unQuit] = await Promise.all([
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
          suijian.events.onNoteCreated((note) => {
            setCurrentNote(note)
            setView('editor')
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
                if (!ok) return
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
          unCreated()
          unHide()
          unQuit()
          return
        }

        unlistenFns = [unFocus, unReqNew, unCreated, unHide, unQuit]

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
        } catch {}
      }
    }
  }, [])

  // scope/标签/初始化/恢复模式变化即时刷新；searchQuery 由下方防抖 effect 处理。
  // 守卫与 deps 必须完整：恢复模式不得发 notes.list（app-workflow.test.tsx:317 护栏）
  useEffect(() => {
    if (isInitialized && !recoveryStatus?.isRecovery) {
      refreshNotesRef.current()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isInitialized, recoveryStatus?.isRecovery, scope, selectedTagId])

  // 搜索防抖：每按键不再直发 IPC+SQLite 查询
  useEffect(() => {
    if (!isInitialized || recoveryStatus?.isRecovery) return
    const t = setTimeout(() => refreshNotesRef.current(), 250)
    return () => clearTimeout(t)
  }, [searchQuery, isInitialized, recoveryStatus?.isRecovery])

  // Normalization helpers for keyboard events
  // 三个配置快捷键的归一化结果按设置缓存，避免每次按键处理重复解析
  const shortcutCombos = useMemo(
    () => ({
      newNote: normalizeShortcutSetting(settings?.shortcutNewNote || 'Ctrl+N'),
      back: normalizeShortcutSetting(settings?.shortcutBackToSearch || 'Ctrl+E'),
      dismiss: normalizeShortcutSetting(settings?.shortcutDismiss || 'Escape'),
    }),
    [settings?.shortcutNewNote, settings?.shortcutBackToSearch, settings?.shortcutDismiss]
  )

  // 应用级保留快捷键：编辑器正文中命中时必须放行给 window 全局处理器（Editor 的 handleDOMEvents 消费）。
  // 不硬编码 ESCAPE：输入法转换中 Escape 是系统取消键，无条件放行会造成误返回；
  // 需要 Escape 在正文生效时由用户把 dismiss 配置为 Escape（由 App 分支 1 的 bare-Escape 段承接）
  const reservedShortcuts = useMemo(() => {
    const set = new Set<string>()
    if (shortcutCombos.back) set.add(shortcutCombos.back)
    if (shortcutCombos.newNote) set.add(shortcutCombos.newNote)
    if (shortcutCombos.dismiss) set.add(shortcutCombos.dismiss)
    return set
  }, [shortcutCombos])

  const isAppReservedShortcut = useCallback(
    (e: KeyboardEvent): boolean => {
      if (e.isComposing || e.keyCode === 229) return false // IME 进行中绝不拦截
      const combo = normalizeCombo(e)
      return combo !== '' && reservedShortcuts.has(combo)
    },
    [reservedShortcuts]
  )

  // Global Keyboard Handlers
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.isComposing || e.defaultPrevented) {
        return
      }

      const { newNote: shortcutNewNote, back: shortcutBack, dismiss: shortcutDismiss } = shortcutCombos

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
          if (shortcutDismiss === 'ESCAPE') {
            e.preventDefault()
            suijian.window.hide().catch((err) => console.error('隐藏窗口失败:', err))
            return
          }
        }
      }

      // 2. Custom Dismiss shortcut
      if (currentCombo === shortcutDismiss) {
        e.preventDefault()
        if (view === 'editor' || view === 'settings') {
          handleBackToSearch()
          return
        }
        if (view === 'search') {
          suijian.window.hide().catch((err) => console.error('隐藏窗口失败:', err))
          return
        }
      }

      // 3. Return to search shortcut (strictly match configured shortcut)
      if (currentCombo === shortcutBack) {
        e.preventDefault()
        if (view !== 'search') {
          handleBackToSearch()
        }
        return
      }

      // 4. New Note shortcut (strictly match configured shortcut)
      if (currentCombo === shortcutNewNote) {
        e.preventDefault()
        handleCreateNote(true)
        return
      }

      // 5. Arrow Up / Down / Enter / Space in Search view
      if (view === 'search' && !isTagModalOpen && !confirmState.isOpen) {
        const target = e.target as HTMLElement
        const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA'

        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setFocusedIndex((prev) => (prev < notes.length - 1 ? prev + 1 : prev))
        } else if (e.key === 'ArrowUp') {
          e.preventDefault()
          setFocusedIndex((prev) => (prev > 0 ? prev - 1 : 0))
        } else if (e.key === 'Enter') {
          if (notes.length > 0 && focusedIndex >= 0 && focusedIndex < notes.length) {
            e.preventDefault()
            setCurrentNote(notes[focusedIndex])
            setView('editor')
          } else if (searchQuery.trim() && isInput) {
            e.preventDefault()
            handleCreateNote(false, searchQuery.trim())
          }
        } else if (e.key === ' ' && !isInput) {
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
  const handleToggleCheck = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

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
            dismissShortcut={settings?.shortcutDismiss || 'Escape'}
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
            onSelectNote={(note) => {
              setCurrentNote(note)
              setView('editor')
            }}
            onToggleCheck={handleToggleCheck}
            onFocusIndex={setFocusedIndex}
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
          backShortcutText={settings?.shortcutBackToSearch || 'Ctrl+E'}
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
          onDataRestored={() => {
            setCurrentNote(null)
            setView('search')
            setSelectedIds(new Set())
            setSelectedTagId(null)
            setSearchQuery('')
            setFocusedIndex(0)
            setFocusTrigger((p) => p + 1)

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
          <span className="kbd-shortcut">{settings?.shortcutDismiss || 'Escape'}</span> 隐藏
        </span>
        <span>随笺 v0.1.0</span>
      </div>
    </div>
  )
}

export default App
