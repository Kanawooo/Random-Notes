import '@testing-library/jest-dom'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'

import { App } from '../../src/App'
import { suijian } from '../../src/lib/api'
import type { Note, Tag, AppSettings } from '../../src/types'

vi.mock('../../src/lib/api', () => ({
  suijian: {
    notes: {
      list: vi.fn(),
      search: vi.fn(),
      get: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      pin: vi.fn(),
      archive: vi.fn(),
      unarchive: vi.fn(),
      trash: vi.fn(),
      restore: vi.fn(),
      trashMany: vi.fn(),
      deletePermanentlyMany: vi.fn(),
      emptyTrash: vi.fn()
    },
    tags: {
      list: vi.fn(),
      create: vi.fn(),
      rename: vi.fn(),
      delete: vi.fn(),
      assign: vi.fn()
    },
    attachments: {
      addFromClipboard: vi.fn(),
      remove: vi.fn(),
      getUrl: vi.fn()
    },
    backup: {
      export: vi.fn(),
      restore: vi.fn()
    },
    settings: {
      getAll: vi.fn(),
      get: vi.fn(),
      update: vi.fn(),
      getHotkeyStatus: vi.fn(() => Promise.resolve({ registered: true, currentHotkey: 'Ctrl+Space' })),
      registerHotkey: vi.fn()
    },
    window: {
      hide: vi.fn(),
      confirmHide: vi.fn(),
      show: vi.fn(),
      getState: vi.fn(),
      updateState: vi.fn()
    },
    app: {
      getInfo: vi.fn(),
      getRecoveryStatus: vi.fn(() => Promise.resolve({ isRecovery: false, dbPath: '', userDataPath: '' })),
      notifyRendererReady: vi.fn(() => Promise.resolve()),
      openUserDataFolder: vi.fn(),
      confirmQuit: vi.fn(),
      quit: vi.fn()
    },
    events: {
      onFocusSearch: vi.fn(() => Promise.resolve(() => {})),
      onRequestNewNote: vi.fn(() => Promise.resolve(() => {})),
      onNoteCreated: vi.fn(() => Promise.resolve(() => {})),
      onRequestHide: vi.fn(() => Promise.resolve(() => {})),
      onRequestQuit: vi.fn(() => Promise.resolve(() => {})),
      onHotkeyStatus: vi.fn(() => Promise.resolve(() => {}))
    }
  }
}))

describe('App Workflow & State Management', () => {
  let initialNotes: Note[]
  let availableTags: Tag[]
  let initialSettings: AppSettings

  beforeEach(() => {
    vi.clearAllMocks()

    initialNotes = [
      {
        id: '11111111-1111-4111-8111-111111111111',
        title: '置顶便签',
        content_json: JSON.stringify({ type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: '置顶内容' }] }] }),
        plain_text: '置顶内容',
        title_manually_edited: true,
        is_pinned: true,
        archived_at: null,
        deleted_at: null,
        revision: 1,
        created_at: '2026-01-01T00:00:00.000Z',
        updated_at: '2026-01-01T00:00:00.000Z',
        tags: [{ id: 'tag-1', name: '工作', normalized_name: '工作', color: '#6366f1', created_at: '' }]
      },
      {
        id: '22222222-2222-4222-8222-222222222222',
        title: '普通便签',
        content_json: JSON.stringify({ type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: '普通内容' }] }] }),
        plain_text: '普通内容',
        title_manually_edited: false,
        is_pinned: false,
        archived_at: null,
        deleted_at: null,
        revision: 1,
        created_at: '2026-01-02T00:00:00.000Z',
        updated_at: '2026-01-02T00:00:00.000Z',
        tags: []
      }
    ]

    availableTags = [
      { id: 'tag-1', name: '工作', normalized_name: '工作', color: '#6366f1', created_at: '' }
    ]

    initialSettings = {
      hotkey: 'Alt+Space',
      launchAtLogin: false,
      autoHideOnBlur: true,
      shortcutNewNote: 'Ctrl+N',
      shortcutBackToSearch: 'Ctrl+E',
      shortcutDismiss: 'Escape',
      windowBounds: null
    }


    vi.mocked(suijian.settings.getAll).mockResolvedValue(initialSettings)
    vi.mocked(suijian.settings.getHotkeyStatus).mockResolvedValue({ registered: true, currentHotkey: 'Ctrl+Space' })
    vi.mocked(suijian.app.getRecoveryStatus).mockResolvedValue({ isRecovery: false, dbPath: '', userDataPath: '' })
    vi.mocked(suijian.tags.list).mockResolvedValue(availableTags)
    vi.mocked(suijian.notes.list).mockResolvedValue(initialNotes)
    vi.mocked(suijian.notes.search).mockImplementation(async (q) =>
      initialNotes.filter((n) => n.title.includes(q) || n.plain_text.includes(q))
    )
    vi.mocked(suijian.notes.create).mockImplementation(async (input) => ({
      id: '33333333-3333-4333-8333-333333333333',
      title: input.title || '未命名便签',
      content_json: JSON.stringify({ type: 'doc', content: [{ type: 'paragraph' }] }),
      plain_text: '',
      title_manually_edited: false,
      is_pinned: false,
      archived_at: null,
      deleted_at: null,
      revision: 0,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      tags: []
    }))
  })

  it('renders initial note list and handles scope switching', async () => {
    render(<App />)

    await waitFor(() => {
      expect(screen.getByText('置顶便签')).toBeInTheDocument()
      expect(screen.getByText('普通便签')).toBeInTheDocument()
    })

    // Switch to trash scope
    const trashTab = screen.getByText(/回收站/)
    fireEvent.click(trashTab)

    await waitFor(() => {
      expect(suijian.notes.list).toHaveBeenCalledWith('trash')
    })
  })

  it('creates new note and opens editor', async () => {
    render(<App />)

    await waitFor(() => {
      expect(screen.getByText('置顶便签')).toBeInTheDocument()
    })

    const newBtn = screen.getByRole('button', { name: /\+ 新建/i })
    fireEvent.click(newBtn)

    await waitFor(() => {
      expect(suijian.notes.create).toHaveBeenCalledWith({
        title: undefined,
        reuse_empty_draft: true
      })
    })
  })

  it('selects notes and executes batch trash', async () => {
    vi.mocked(suijian.notes.trashMany).mockResolvedValue({ affectedCount: 2 })

    render(<App />)

    await waitFor(() => {
      expect(screen.getByText('置顶便签')).toBeInTheDocument()
    })

    // Click checkbox on first note card to activate batch mode
    const checkboxes = screen.getAllByRole('checkbox')
    expect(checkboxes.length).toBeGreaterThan(0)
    fireEvent.click(checkboxes[0])

    // Batch bar appears
    await waitFor(() => {
      expect(screen.getByText(/已选 1 项/i)).toBeInTheDocument()
    })

    // Click select all
    const selectAll = screen.getByText(/全选当前/i)
    fireEvent.click(selectAll)
    expect(screen.getByText(/已选 2 项/i)).toBeInTheDocument()

    // Click batch trash button
    const trashBatchBtn = screen.getByRole('button', { name: /移入回收站/i })
    fireEvent.click(trashBatchBtn)

    // Confirm dialog appears
    await waitFor(() => {
      expect(screen.getByText(/确定要将选中的 2 篇便签移入回收站吗？/i)).toBeInTheDocument()
    })

    const confirmBtn = screen.getByRole('button', { name: /确定/i })
    fireEvent.click(confirmBtn)

    await waitFor(() => {
      expect(suijian.notes.trashMany).toHaveBeenCalledWith([
        '11111111-1111-4111-8111-111111111111',
        '22222222-2222-4222-8222-222222222222'
      ])
    })
  })

  it('opens and closes settings modal', async () => {
    render(<App />)

    await waitFor(() => {
      expect(screen.getByTitle(/设置/i)).toBeInTheDocument()
    })

    fireEvent.click(screen.getByTitle(/设置/i))

    await waitFor(() => {
      expect(screen.getByText('快捷键设置')).toBeInTheDocument()
    })

    // Close button
    const closeBtn = screen.getByRole('button', { name: '关闭' })
    fireEvent.click(closeBtn)

    await waitFor(() => {
      expect(screen.queryByText('快捷键设置')).not.toBeInTheDocument()
    })
  })

  it('bare Escape closes settings modal even if shortcutDismiss is customized', async () => {
    vi.mocked(suijian.settings.getAll).mockResolvedValue({
      ...initialSettings,
      shortcutDismiss: 'Ctrl+W'
    })

    render(<App />)

    await waitFor(() => {
      expect(screen.getByTitle(/设置/i)).toBeInTheDocument()
    })

    fireEvent.click(screen.getByTitle(/设置/i))

    await waitFor(() => {
      expect(screen.getByText('快捷键设置')).toBeInTheDocument()
    })

    // Bare Escape closes modal
    fireEvent.keyDown(window, { key: 'Escape' })

    await waitFor(() => {
      expect(screen.queryByText('快捷键设置')).not.toBeInTheDocument()
    })
  })

  it('displays recovery banner and blocks note operations when in recovery mode', async () => {
    vi.mocked(suijian.app.getRecoveryStatus).mockResolvedValue({
      isRecovery: true,
      error: '数据库格式不匹配',
      dbPath: 'C:/mock/db.sqlite',
      userDataPath: 'C:/mock'
    })

    render(<App />)

    await waitFor(() => {
      expect(screen.getByText(/数据只读保护模式/i)).toBeInTheDocument()
      expect(screen.getByText(/数据库格式不匹配/i)).toBeInTheDocument()
    })

    // Notes should not have been loaded
    expect(suijian.notes.list).not.toHaveBeenCalled()

    // New note should display warning banner and not call suijian.notes.create
    const newBtn = screen.getByRole('button', { name: /\+ 新建/i })
    fireEvent.click(newBtn)

    await waitFor(() => {
      expect(screen.getByText(/当前处于数据只读保护模式，无法新建便签/i)).toBeInTheDocument()
    })
    expect(suijian.notes.create).not.toHaveBeenCalled()
  })

  it('does not trigger global shortcuts when typing in shortcut input field', async () => {
    render(<App />)

    await waitFor(() => {
      expect(screen.getByTitle(/设置/i)).toBeInTheDocument()
    })

    fireEvent.click(screen.getByTitle(/设置/i))

    await waitFor(() => {
      expect(screen.getByText('快捷键设置')).toBeInTheDocument()
    })

    const newNoteInput = screen.getByLabelText('新建便签快捷键')
    fireEvent.keyDown(newNoteInput, { key: 'n', ctrlKey: true })

    expect(newNoteInput).toHaveValue('Ctrl+N')
    expect(suijian.notes.create).not.toHaveBeenCalled()
  })

  it('honors reconfigured shortcut and ignores old shortcut', async () => {
    vi.mocked(suijian.settings.getAll).mockResolvedValue({
      ...initialSettings,
      shortcutNewNote: 'Ctrl+B'
    })

    render(<App />)

    await waitFor(() => {
      expect(screen.getByText('置顶便签')).toBeInTheDocument()
    })

    // Default Ctrl+N should not trigger
    fireEvent.keyDown(window, { key: 'n', ctrlKey: true })
    expect(suijian.notes.create).not.toHaveBeenCalled()

    // Configured Ctrl+B should trigger
    fireEvent.keyDown(window, { key: 'b', ctrlKey: true })
    await waitFor(() => {
      expect(suijian.notes.create).toHaveBeenCalledWith({
        title: undefined,
        reuse_empty_draft: true
      })
    })
  })

  it('blocks navigation to settings and preserves note when save fails during flush', async () => {
    vi.mocked(suijian.settings.getHotkeyStatus).mockResolvedValue({
      registered: false,
      currentHotkey: 'Ctrl+Space',
      error: '热键冲突警告'
    })

    render(<App />)

    await waitFor(() => {
      expect(screen.getByText('置顶便签')).toBeInTheDocument()
      expect(screen.getByText(/全局呼出快捷键.*注册失败/i)).toBeInTheDocument()
    })

    // Click note to enter editor
    fireEvent.click(screen.getByText('置顶便签'))

    await waitFor(() => {
      expect(screen.getByLabelText('便签标题输入框')).toBeInTheDocument()
    })

    // Cause update to fail
    vi.mocked(suijian.notes.update).mockRejectedValueOnce(new Error('保存失败: 磁盘写入错误'))

    // Change title so changes are scheduled
    const titleInput = screen.getByLabelText('便签标题输入框')
    fireEvent.change(titleInput, { target: { value: '修改后的标题' } })

    // Click the "设置" button in warning banner (aria-label: 前往设置热键)
    const toSettingsBtn = screen.getByRole('button', { name: '前往设置热键' })
    fireEvent.click(toSettingsBtn)

    // Should stay in editor and show error banner
    await waitFor(() => {
      expect(
        screen.getByText(/便签正在保存中或保存失败，无法进入设置/i)
      ).toBeInTheDocument()
    })
    expect(screen.getByLabelText('便签标题输入框')).toBeInTheDocument()
    expect(screen.queryByText('快捷键设置')).not.toBeInTheDocument()
  })

  it('synchronizes tags and resets selection when tag is modified or deleted in settings', async () => {
    vi.mocked(suijian.tags.delete).mockResolvedValue(undefined)
    vi.mocked(suijian.tags.list).mockResolvedValue(availableTags)

    render(<App />)

    await waitFor(() => {
      expect(screen.getByText('置顶便签')).toBeInTheDocument()
      expect(screen.getByRole('button', { name: '#工作' })).toBeInTheDocument()
    })

    // Select the tag filter in search view
    fireEvent.click(screen.getByRole('button', { name: '#工作' }))
    expect(screen.getByRole('button', { name: '#工作' })).toHaveClass('active')

    // Open settings
    fireEvent.click(screen.getByTitle(/设置/i))
    await waitFor(() => {
      expect(screen.getByText('标签管理')).toBeInTheDocument()
    })

    // Once tag is deleted, next tags.list returns empty
    vi.mocked(suijian.tags.list).mockResolvedValue([])

    // Find delete button for tag-1 and click it
    const deleteBtn = screen.getByLabelText('删除标签 工作')
    fireEvent.click(deleteBtn)

    await waitFor(() => {
      expect(suijian.tags.delete).toHaveBeenCalledWith('tag-1')
    })

    // Return to search view
    const backBtn = screen.getByRole('button', { name: '关闭' })
    fireEvent.click(backBtn)

    await waitFor(() => {
      expect(screen.getByText('置顶便签')).toBeInTheDocument()
    })
    // Deleted tag should no longer be present in search filter
    expect(screen.queryByRole('button', { name: '#工作' })).not.toBeInTheDocument()
  })
})

