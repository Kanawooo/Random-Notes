export interface BackupExportResult {
  canceled: boolean
  filePath?: string
  noteCount?: number
  attachmentCount?: number
}

export interface BackupInspectResult {
  canceled: boolean
  token?: string
  noteCount?: number
  tagCount?: number
  attachmentCount?: number
  totalByteSize?: number
  fileName?: string
}

export interface BackupRestoreResult {
  canceled: boolean
  restoredNoteCount?: number
  restoredTagCount?: number
  restoredAttachmentCount?: number
}

export interface BackupFileEntry {
  path: string
  byteSize: number
  sha256: string
}

export interface BackupTagItem {
  id: string
  name: string
  color: string
  created_at?: string
}

export interface BackupAttachmentItem {
  id: string
  relative_path: string
  mime_type: string
  byte_size: number
  width: number | null
  height: number | null
  sha256: string
  created_at: string
}

export interface BackupNoteData {
  id: string
  title: string
  content_json: string
  plain_text: string
  title_manually_edited: boolean
  is_pinned: boolean
  archived_at: string | null
  deleted_at: string | null
  revision: number
  created_at: string
  updated_at: string
  tags: BackupTagItem[]
  attachments?: BackupAttachmentItem[]
}

export interface BackupManifest {
  version: number
  appVersion: string
  exportedAt: string
  tags?: BackupTagItem[]
  files: BackupFileEntry[]
}
