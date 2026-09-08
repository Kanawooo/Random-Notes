export interface WindowBounds {
  x: number
  y: number
  width: number
  height: number
}

export interface AppSettings {
  hotkey: string
  shortcutNewNote: string
  shortcutBackToSearch: string
  shortcutDismiss: string
  launchAtLogin: boolean
  autoHideOnBlur: boolean
  windowBounds: WindowBounds | null
}

export interface HotkeyStatus {
  registered: boolean
  currentHotkey: string
  requestedHotkey?: string
  error?: string
  recommendedHotkey?: string
}


export interface AppInfo {
  name: string
  version: string
  userDataPath: string
  dbPath: string
}

export interface RecoveryStatus {
  isRecovery: boolean
  error?: string
  dbPath: string
  userDataPath: string
}

