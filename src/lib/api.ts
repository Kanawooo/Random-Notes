import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type {
  Note,
  NoteScope,
  CreateNoteInput,
  UpdateNoteInput,
  BatchResult,
  Tag,
  CreateTagInput,
  RenameTagInput,
  AssignTagsInput,
  Attachment,
  AppSettings,
  HotkeyStatus,
  WindowBounds,
  AppInfo,
  BackupExportResult,
  BackupRestoreResult
} from '../types'

export const suijian = {
  notes: {
    list: (scope?: NoteScope, limit?: number): Promise<Note[]> =>
      invoke<Note[]>('notes_list', { scope, limit }),
    search: (query: string, scope?: NoteScope, limit?: number): Promise<Note[]> =>
      invoke<Note[]>('notes_search', { query, scope, limit }),
    get: (id: string): Promise<Note | null> =>
      invoke<Note | null>('notes_get', { id }),
    create: (input: CreateNoteInput): Promise<Note> =>
      invoke<Note>('notes_create', { input }),
    update: (input: UpdateNoteInput): Promise<Note> =>
      invoke<Note>('notes_update', { input }),
    pin: (id: string, isPinned: boolean): Promise<Note> =>
      invoke<Note>('notes_pin', { id, isPinned }),
    archive: (id: string): Promise<Note> =>
      invoke<Note>('notes_archive', { id }),
    unarchive: (id: string): Promise<Note> =>
      invoke<Note>('notes_unarchive', { id }),
    trash: (id: string): Promise<Note> =>
      invoke<Note>('notes_trash', { id }),
    restore: (id: string): Promise<Note> =>
      invoke<Note>('notes_restore', { id }),
    trashMany: (ids: string[]): Promise<BatchResult> =>
      invoke<BatchResult>('notes_trash_many', { ids }),
    deletePermanentlyMany: (ids: string[]): Promise<BatchResult> =>
      invoke<BatchResult>('notes_delete_permanently_many', { ids }),
    emptyTrash: (): Promise<BatchResult> =>
      invoke<BatchResult>('notes_empty_trash')
  },
  tags: {
    list: (): Promise<Tag[]> =>
      invoke<Tag[]>('tags_list'),
    create: (input: CreateTagInput): Promise<Tag> =>
      invoke<Tag>('tags_create', { input }),
    rename: (input: RenameTagInput): Promise<Tag> =>
      invoke<Tag>('tags_rename', { input }),
    delete: (id: string): Promise<void> =>
      invoke<void>('tags_delete', { id }),
    assign: (input: AssignTagsInput): Promise<void> =>
      invoke<void>('tags_assign', { input })
  },
  attachments: {
    addFromClipboard: (noteId: string): Promise<Attachment> =>
      invoke<Attachment>('attachments_add_from_clipboard', { noteId }),
    remove: (id: string): Promise<void> =>
      invoke<void>('attachments_remove', { id }),
    getUrl: (id: string): Promise<string> =>
      invoke<string>('attachments_get_url', { id })
  },
  backup: {
    export: (defaultFilename?: string): Promise<BackupExportResult> =>
      invoke<BackupExportResult>('backup_export', { defaultFilename }),
    inspectSelect: (): Promise<import('../types').BackupInspectResult> =>
      invoke<import('../types').BackupInspectResult>('backup_inspect_select'),
    restoreConfirm: (token: string): Promise<BackupRestoreResult> =>
      invoke<BackupRestoreResult>('backup_restore_confirm', { token })
  },
  settings: {
    getAll: (): Promise<AppSettings> =>
      invoke<AppSettings>('settings_get_all'),
    get: (key: string): Promise<string | null> =>
      invoke<string | null>('settings_get', { key }),
    update: (key: string, value: unknown): Promise<AppSettings> =>
      invoke<AppSettings>('settings_update', { key, value }),
    getHotkeyStatus: (): Promise<HotkeyStatus> =>
      invoke<HotkeyStatus>('settings_get_hotkey_status'),
    registerHotkey: (hotkey: string): Promise<HotkeyStatus> =>
      invoke<HotkeyStatus>('settings_register_hotkey', { hotkey }),
    updateActionShortcuts: (args: {
      shortcutNewNote: string
      shortcutBackToSearch: string
      shortcutDismiss: string
    }): Promise<AppSettings> =>
      invoke<AppSettings>('settings_update_action_shortcuts', {
        shortcutNewNote: args.shortcutNewNote,
        shortcutBackToSearch: args.shortcutBackToSearch,
        shortcutDismiss: args.shortcutDismiss
      })
  },
  window: {
    hide: (): Promise<void> =>
      invoke<void>('window_hide'),
    confirmHide: (): Promise<void> =>
      invoke<void>('window_confirm_hide'),
    show: (): Promise<void> =>
      invoke<void>('window_show'),
    setDialogOpen: (open: boolean): Promise<void> =>
      invoke<void>('set_dialog_open', { open }),
    getState: (): Promise<WindowBounds | null> =>
      invoke<WindowBounds | null>('window_get_state'),
    updateState: (bounds: WindowBounds): Promise<void> =>
      invoke<void>('window_update_state', { bounds })
  },
  app: {
    getInfo: (): Promise<AppInfo> =>
      invoke<AppInfo>('app_get_info'),
    getRecoveryStatus: (): Promise<import('../types').RecoveryStatus> =>
      invoke<import('../types').RecoveryStatus>('app_get_recovery_status'),
    openUserDataFolder: (): Promise<void> =>
      invoke<void>('app_open_user_data_folder'),
    quit: (): Promise<void> =>
      invoke<void>('app_quit'),
    confirmQuit: (): Promise<void> =>
      invoke<void>('app_confirm_quit'),
    openExternal: (url: string): Promise<void> =>
      invoke<void>('app_open_external', { url }),
    notifyRendererReady: (): Promise<void> =>
      invoke<void>('renderer_ready')
  },
  events: {
    onNoteCreated: (cb: (note: Note) => void): Promise<UnlistenFn> =>
      listen<Note>('event:note-created', (e) => cb(e.payload)),
    onRequestNewNote: (cb: () => void): Promise<UnlistenFn> =>
      listen<void>('event:request-new-note', () => cb()),
    onFocusSearch: (cb: () => void): Promise<UnlistenFn> =>
      listen<void>('event:focus-search', () => cb()),
    onRequestHide: (cb: () => void): Promise<UnlistenFn> =>
      listen<void>('event:request-hide', () => cb()),
    onRequestQuit: (cb: () => void): Promise<UnlistenFn> =>
      listen<void>('event:request-quit', () => cb()),
    onHotkeyStatus: (cb: (status: HotkeyStatus) => void): Promise<UnlistenFn> =>
      listen<HotkeyStatus>('event:hotkey-status', (e) => cb(e.payload))
  }
}

// Attach to window.suijian for compatibility
if (typeof window !== 'undefined') {
  ;(window as unknown as { suijian: typeof suijian }).suijian = suijian
}
