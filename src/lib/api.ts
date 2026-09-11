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
  AppInfo,
  BackupExportResult,
  BackupRestoreResult,
  CloudBackupConfig,
  CloudBackupConfigInput,
  CloudBackupFile,
  CloudBackupRunResult,
  CloudBackupTestResult
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
    addFromBytes: (noteId: string, dataBase64: string): Promise<Attachment> =>
      invoke<Attachment>('attachments_add_from_bytes', { noteId, data: dataBase64 })
  },
  backup: {
    export: (defaultFilename?: string): Promise<BackupExportResult> =>
      invoke<BackupExportResult>('backup_export', { defaultFilename }),
    inspectSelect: (): Promise<import('../types').BackupInspectResult> =>
      invoke<import('../types').BackupInspectResult>('backup_inspect_select'),
    restoreConfirm: (token: string): Promise<BackupRestoreResult> =>
      invoke<BackupRestoreResult>('backup_restore_confirm', { token })
  },
  cloudBackup: {
    configGet: (): Promise<CloudBackupConfig> =>
      invoke<CloudBackupConfig>('cloud_backup_config_get'),
    configUpdate: (input: CloudBackupConfigInput): Promise<CloudBackupConfig> =>
      invoke<CloudBackupConfig>('cloud_backup_config_update', { input }),
    testConnection: (args: {
      account: string
      password?: string
      davUrl: string
    }): Promise<CloudBackupTestResult> =>
      invoke<CloudBackupTestResult>('cloud_backup_test_connection', {
        account: args.account,
        password: args.password,
        davUrl: args.davUrl
      }),
    run: (): Promise<CloudBackupRunResult> =>
      invoke<CloudBackupRunResult>('cloud_backup_run'),
    list: (): Promise<CloudBackupFile[]> =>
      invoke<CloudBackupFile[]>('cloud_backup_list'),
    restorePrepare: (fileName: string): Promise<import('../types').BackupInspectResult> =>
      invoke<import('../types').BackupInspectResult>('cloud_backup_restore_prepare', { fileName }),
    restoreCancel: (): Promise<void> => invoke<void>('cloud_backup_restore_cancel')
  },
  settings: {
    getAll: (): Promise<AppSettings> =>
      invoke<AppSettings>('settings_get_all'),
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
    confirmHide: (): Promise<void> =>
      invoke<void>('window_confirm_hide'),
    setUnsavedError: (hasError: boolean): Promise<void> =>
      invoke<void>('window_set_unsaved_error', { hasError })
  },
  app: {
    getInfo: (): Promise<AppInfo> =>
      invoke<AppInfo>('app_get_info'),
    getRecoveryStatus: (): Promise<import('../types').RecoveryStatus> =>
      invoke<import('../types').RecoveryStatus>('app_get_recovery_status'),
    openUserDataFolder: (): Promise<void> =>
      invoke<void>('app_open_user_data_folder'),
    confirmQuit: (): Promise<void> =>
      invoke<void>('app_confirm_quit'),
    openExternal: (url: string): Promise<void> =>
      invoke<void>('app_open_external', { url }),
    notifyRendererReady: (): Promise<void> =>
      invoke<void>('renderer_ready')
  },
  events: {
    onRequestNewNote: (cb: () => void): Promise<UnlistenFn> =>
      listen<void>('event:request-new-note', () => cb()),
    onFocusSearch: (cb: () => void): Promise<UnlistenFn> =>
      listen<void>('event:focus-search', () => cb()),
    onRequestHide: (cb: () => void): Promise<UnlistenFn> =>
      listen<void>('event:request-hide', () => cb()),
    onRequestQuit: (cb: () => void): Promise<UnlistenFn> =>
      listen<void>('event:request-quit', () => cb())
  }
}

// Attach to window.suijian for compatibility
if (typeof window !== 'undefined') {
  ;(window as unknown as { suijian: typeof suijian }).suijian = suijian
}
