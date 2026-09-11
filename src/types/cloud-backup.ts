// 云端备份（坚果云 WebDAV）wire 类型镜像：严格对照 src-tauri/src/db/models.rs
// 中 CloudBackup* DTO 的 serde rename（camelCase）；Option 出参字段序列化为 null。

export type CloudBackupInterval = 'daily' | 'every3days' | 'weekly'

export type CloudBackupRunStatus = 'uploaded' | 'nochange'

export interface CloudBackupConfig {
  enabled: boolean
  interval: CloudBackupInterval
  keepCount: number
  account: string
  davUrl: string
  hasPassword: boolean
  lastSuccessAt: string | null
  lastError: string | null
}

export interface CloudBackupConfigInput {
  enabled: boolean
  interval: CloudBackupInterval
  keepCount: number
  account: string
  davUrl: string
  // 留空（undefined 或空串）表示沿用已存密码；密码不回传（出参只有 hasPassword）
  password?: string
}

export interface CloudBackupFile {
  fileName: string
  sizeBytes: number
  modifiedAt: string
}

export interface CloudBackupRunResult {
  status: CloudBackupRunStatus
  fileName: string | null
  sizeBytes: number | null
  message: string
}

export interface CloudBackupTestResult {
  ok: boolean
  message: string
}
