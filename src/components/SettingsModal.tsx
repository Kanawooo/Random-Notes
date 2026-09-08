import React, { useState, useEffect } from 'react'
import { suijian } from '../lib/api'
import type { AppSettings, BackupInspectResult } from '../types'
import { UiIcon } from './UiIcon'

interface SettingsModalProps {
  isOpen: boolean
  onClose: () => void
  onSettingsChanged?: (settings: AppSettings) => void
  onDataRestored?: () => void
  onBeforeRestore?: () => Promise<boolean>
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  onSettingsChanged,
  onDataRestored,
  onBeforeRestore
}) => {
  const [hotkeyInput, setHotkeyInput] = useState('')
  const [newNoteInput, setNewNoteInput] = useState('')
  const [backSearchInput, setBackSearchInput] = useState('')
  const [dismissInput, setDismissInput] = useState('')
  const [launchAtLogin, setLaunchAtLogin] = useState(false)
  const [autoHideOnBlur, setAutoHideOnBlur] = useState(true)
  const [statusMsg, setStatusMsg] = useState<{ text: string; isError?: boolean } | null>(null)
  const [isProcessingBackup, setIsProcessingBackup] = useState(false)
  const [inspectResult, setInspectResult] = useState<BackupInspectResult | null>(null)
  const [isRestoring, setIsRestoring] = useState(false)

  useEffect(() => {
    if (isOpen) {
      suijian.settings.getAll().then((s) => {
        setHotkeyInput(s.hotkey)
        setNewNoteInput(s.shortcutNewNote)
        setBackSearchInput(s.shortcutBackToSearch)
        setDismissInput(s.shortcutDismiss)
        setLaunchAtLogin(s.launchAtLogin)
        setAutoHideOnBlur(s.autoHideOnBlur)
      })
      setStatusMsg(null)
      setInspectResult(null)
    }
  }, [isOpen])

  if (!isOpen) return null

  const handleShortcutKeyDown = (
    e: React.KeyboardEvent<HTMLInputElement>,
    setter: (val: string) => void
  ) => {
    if (e.key === 'Tab') return
    if (e.key === 'Backspace' || e.key === 'Delete') {
      e.preventDefault()
      setter('')
      return
    }
    if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) {
      return
    }
    e.preventDefault()
    const parts: string[] = []
    if (e.ctrlKey) parts.push('Ctrl')
    if (e.altKey) parts.push('Alt')
    if (e.shiftKey) parts.push('Shift')
    if (e.metaKey) parts.push('Super')

    let keyName = e.key
    if (keyName === ' ') keyName = 'Space'
    else if (keyName === 'Escape') keyName = 'Escape'
    else if (keyName.length === 1) keyName = keyName.toUpperCase()

    parts.push(keyName)
    setter(parts.join('+'))
  }

  const handleSaveHotkey = async () => {
    try {
      const res = await suijian.settings.registerHotkey(hotkeyInput)
      if (res.registered) {
        setStatusMsg({ text: '全局热键保存并注册成功！' })
        const all = await suijian.settings.getAll()
        onSettingsChanged?.(all)
      } else {
        setStatusMsg({
          text: `${res.error || '注册失败'}。建议使用: ${res.recommendedHotkey || 'Ctrl+Shift+Space'}`,
          isError: true
        })
      }
    } catch (err: unknown) {
      setStatusMsg({ text: String(err), isError: true })
    }
  }

  const handleSaveShortcuts = async () => {
    try {
      const updated = await suijian.settings.updateActionShortcuts({
        shortcutNewNote: newNoteInput,
        shortcutBackToSearch: backSearchInput,
        shortcutDismiss: dismissInput
      })
      setStatusMsg({ text: '应用内动作快捷键已更新！' })
      onSettingsChanged?.(updated)
    } catch (err: unknown) {
      setStatusMsg({ text: String(err), isError: true })
    }
  }

  const handleRestoreDefaults = async () => {
    setHotkeyInput('Ctrl+Space')
    setNewNoteInput('Ctrl+N')
    setBackSearchInput('Ctrl+E')
    setDismissInput('Escape')

    try {
      await suijian.settings.updateActionShortcuts({
        shortcutNewNote: 'Ctrl+N',
        shortcutBackToSearch: 'Ctrl+E',
        shortcutDismiss: 'Escape'
      })
      await suijian.settings.registerHotkey('Ctrl+Space')
      const all = await suijian.settings.getAll()
      setStatusMsg({ text: '快捷键已恢复默认设置！' })
      onSettingsChanged?.(all)
    } catch (err: unknown) {
      setStatusMsg({ text: String(err), isError: true })
    }
  }

  const handleToggleLaunchAtLogin = async (checked: boolean) => {
    setLaunchAtLogin(checked)
    try {
      const updated = await suijian.settings.update('launchAtLogin', checked)
      if (onSettingsChanged) onSettingsChanged(updated)
    } catch (err: unknown) {
      setStatusMsg({ text: String(err), isError: true })
    }
  }

  const handleToggleAutoHideOnBlur = async (checked: boolean) => {
    setAutoHideOnBlur(checked)
    try {
      const updated = await suijian.settings.update('autoHideOnBlur', checked)
      if (onSettingsChanged) onSettingsChanged(updated)
    } catch (err: unknown) {
      setStatusMsg({ text: String(err), isError: true })
    }
  }

  const handleExportBackup = async () => {
    setIsProcessingBackup(true)
    setStatusMsg({ text: '正在导出数据包...' })
    try {
      const now = new Date()
      const filename = `suijian-backup-${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, '0')}${String(now.getDate()).padStart(2, '0')}.zip`
      const res = await suijian.backup.export(filename)
      if (res.canceled) {
        setStatusMsg(null)
      } else {
        setStatusMsg({ text: `导出成功！已保存 ${res.noteCount || 0} 篇便签，${res.attachmentCount || 0} 个附件。` })
      }
    } catch (err: unknown) {
      setStatusMsg({ text: `导出失败: ${err}`, isError: true })
    } finally {
      setIsProcessingBackup(false)
    }
  }

  const handleInspectBackup = async () => {
    setIsProcessingBackup(true)
    setStatusMsg(null)
    try {
      const res = await suijian.backup.inspectSelect()
      if (res.canceled) {
        return
      }
      setInspectResult(res)
    } catch (err: unknown) {
      setStatusMsg({ text: `解析备份失败: ${err}`, isError: true })
    } finally {
      setIsProcessingBackup(false)
    }
  }

  const handleConfirmRestore = async () => {
    if (!inspectResult?.token) return
    setIsRestoring(true)
    setStatusMsg({ text: '正在检查并恢复数据，请稍候...' })
    try {
      if (onBeforeRestore) {
        const canRestore = await onBeforeRestore()
        if (!canRestore) {
          setStatusMsg({ text: '当前正在编辑的便签保存未完成或失败，已取消恢复以避免丢失未保存内容', isError: true })
          setIsRestoring(false)
          return
        }
      }
      const res = await suijian.backup.restoreConfirm(inspectResult.token)
      setStatusMsg({
        text: `恢复成功！已恢复 ${res.restoredNoteCount ?? 0} 篇便签，${res.restoredTagCount ?? 0} 个标签，${res.restoredAttachmentCount ?? 0} 个附件。`
      })
      setInspectResult(null)
      onDataRestored?.()
    } catch (err: unknown) {
      setStatusMsg({ text: `恢复失败: ${err}`, isError: true })
    } finally {
      setIsRestoring(false)
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal-card" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h3>设置</h3>
          <button
            className="modal-close-btn"
            onClick={onClose}
            title="关闭"
            aria-label="关闭"
          >
            <UiIcon name="close" size={16} />
            <span className="sr-only">×</span>
          </button>
        </div>

        {statusMsg && (
          <div className={`settings-alert ${statusMsg.isError ? 'alert-error' : 'alert-success'}`}>
            {statusMsg.text}
          </div>
        )}

        <div className="modal-body settings-body">
          {/* Shortcuts Section */}
          <div className="settings-section">
            <h4>快捷键设置</h4>

            <div className="setting-field">
              <label>全局呼出/收起快捷键</label>
              <div className="input-with-button">
                <input
                  type="text"
                  value={hotkeyInput}
                  onChange={(e) => setHotkeyInput(e.target.value)}
                  onKeyDown={(e) => handleShortcutKeyDown(e, setHotkeyInput)}
                  placeholder="Ctrl+Space"
                />
                <button className="btn-secondary" onClick={handleSaveHotkey}>
                  应用
                </button>
              </div>
            </div>

            <div className="setting-field">
              <label>新建便签快捷键</label>
              <input
                type="text"
                value={newNoteInput}
                onChange={(e) => setNewNoteInput(e.target.value)}
                onKeyDown={(e) => handleShortcutKeyDown(e, setNewNoteInput)}
                placeholder="Ctrl+N"
              />
            </div>

            <div className="setting-field">
              <label>返回搜索快捷键</label>
              <input
                type="text"
                value={backSearchInput}
                onChange={(e) => setBackSearchInput(e.target.value)}
                onKeyDown={(e) => handleShortcutKeyDown(e, setBackSearchInput)}
                placeholder="Ctrl+E"
              />
            </div>

            <div className="setting-field">
              <label>取消/隐藏快捷键</label>
              <input
                type="text"
                value={dismissInput}
                onChange={(e) => setDismissInput(e.target.value)}
                onKeyDown={(e) => handleShortcutKeyDown(e, setDismissInput)}
                placeholder="Escape"
              />
            </div>

            <div className="settings-btn-row">
              <button className="btn-secondary" onClick={handleSaveShortcuts}>
                保存动作快捷键
              </button>
              <button className="btn-text" onClick={handleRestoreDefaults}>
                恢复默认快捷键
              </button>
            </div>
          </div>

          {/* Behavior Section */}
          <div className="settings-section">
            <h4>常规行为</h4>
            <label className="checkbox-setting-row">
              <input
                type="checkbox"
                checked={autoHideOnBlur}
                onChange={(e) => handleToggleAutoHideOnBlur(e.target.checked)}
              />
              <span>失去焦点时自动保存并收起面板</span>
            </label>

            <label className="checkbox-setting-row">
              <input
                type="checkbox"
                checked={launchAtLogin}
                onChange={(e) => handleToggleLaunchAtLogin(e.target.checked)}
              />
              <span>登录时启动随笺</span>
            </label>
          </div>

          {/* Backup Section */}
          <div className="settings-section">
            <h4>数据备份与恢复</h4>
            <div className="backup-buttons-row">
              <button
                className="btn-secondary"
                disabled={isProcessingBackup || isRestoring}
                onClick={handleExportBackup}
              >
                导出完整备份 (.zip)
              </button>
              <button
                className="btn-secondary"
                disabled={isProcessingBackup || isRestoring}
                onClick={handleInspectBackup}
              >
                从备份恢复 (.zip)
              </button>
            </div>

            {inspectResult && (
              <div className="restore-confirm-card" style={{ marginTop: '12px', padding: '12px', background: 'rgba(239, 68, 68, 0.08)', border: '1px solid rgba(239, 68, 68, 0.25)', borderRadius: '6px' }}>
                <div style={{ fontWeight: 600, color: '#dc2626', marginBottom: '8px', display: 'flex', alignItems: 'center', gap: '6px' }}>
                  <UiIcon name="warning" size={16} /> 恢复数据确认
                </div>
                <div style={{ fontSize: '13px', lineHeight: '1.6', marginBottom: '8px' }}>
                  准备从 <b>{inspectResult.fileName || '备份文件'}</b> 恢复：<br />
                  • 便签数：{inspectResult.noteCount ?? 0} 篇<br />
                  • 标签数：{inspectResult.tagCount ?? 0} 个<br />
                  • 附件数：{inspectResult.attachmentCount ?? 0} 个
                  {inspectResult.totalByteSize ? ` (${(inspectResult.totalByteSize / 1024 / 1024).toFixed(2)} MB)` : ''}
                </div>
                <div style={{ fontSize: '12px', color: '#b91c1c', marginBottom: '10px' }}>
                  注意：恢复将覆盖现有便签及附件！系统将在开始恢复前自动创建本地安全快照。
                </div>
                <div style={{ display: 'flex', gap: '8px' }}>
                  <button
                    className="btn-secondary"
                    style={{ background: '#dc2626', color: '#ffffff', borderColor: '#dc2626' }}
                    disabled={isRestoring}
                    onClick={handleConfirmRestore}
                  >
                    {isRestoring ? '正在恢复...' : '确认覆盖恢复'}
                  </button>
                  <button
                    className="btn-text"
                    disabled={isRestoring}
                    onClick={() => setInspectResult(null)}
                  >
                    取消
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
