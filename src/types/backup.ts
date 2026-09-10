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
