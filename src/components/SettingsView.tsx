import React, { useState, useEffect, useRef } from 'react'
import { suijian } from '../lib/api'
import type { AppSettings, Tag, BackupInspectResult } from '../types'
import { toErrMsg } from '../lib/errors'
import { normalizeShortcutSetting, detectConflict } from '../lib/shortcuts'

type ShortcutField = 'hotkey' | 'newNote' | 'back' | 'dismiss'

interface SettingsViewProps {
  onSettingsChanged?: (settings: AppSettings) => void
  onTagsChanged?: (deletedTagId?: string) => void
  onDataRestored?: () => void
  onBeforeRestore?: () => Promise<boolean>
}

export const SettingsView: React.FC<SettingsViewProps> = ({
  onSettingsChanged,
  onTagsChanged,
  onDataRestored,
  onBeforeRestore
}) => {
  const [hotkeyInput, setHotkeyInput] = useState('')
  const [newNoteInput, setNewNoteInput] = useState('')
  const [backSearchInput, setBackSearchInput] = useState('')
  const [dismissInput, setDismissInput] = useState('')
  const [launchAtLogin, setLaunchAtLogin] = useState(false)
  const [autoHideOnBlur, setAutoHideOnBlur] = useState(false)

  const [hotkeyMessage, setHotkeyMessage] = useState<string | null>(null)
  const [hotkeySuccess, setHotkeySuccess] = useState<boolean>(true)
  const [actionShortcutMsg, setActionShortcutMsg] = useState<string | null>(null)

  // Tags
  const [tags, setTags] = useState<Tag[]>([])
  const [newTagName, setNewTagName] = useState('')
  const [editingTagId, setEditingTagId] = useState<string | null>(null)
  const [editingTagName, setEditingTagName] = useState('')
  const [tagError, setTagError] = useState<string | null>(null)

  // Backup
  const [backupLoading, setBackupLoading] = useState(false)
  const [backupStatus, setBackupStatus] = useState<'idle' | 'success' | 'error'>('idle')
  const [backupMessage, setBackupMessage] = useState<string | null>(null)
  const [prefError, setPrefError] = useState<string | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  // 录制时的软冲突警告（硬互斥仍由后端保存时拒绝）；单一事实源在 lib/shortcuts
  const [scWarnings, setScWarnings] = useState<Partial<Record<ShortcutField, string | null>>>({})
  const msgTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [inspectResult, setInspectResult] = useState<BackupInspectResult | null>(null)
  const [isRestoring, setIsRestoring] = useState(false)

  const loadSettingsAndTags = async () => {
    try {
      const s = await suijian.settings.getAll()
      setHotkeyInput(s.hotkey)
      setNewNoteInput(s.shortcutNewNote)
      setBackSearchInput(s.shortcutBackToSearch)
      setDismissInput(s.shortcutDismiss)
      setLaunchAtLogin(s.launchAtLogin)
      setAutoHideOnBlur(s.autoHideOnBlur)
      setScWarnings({})

      const status = await suijian.settings.getHotkeyStatus()
      if (!status.registered) {
        setHotkeySuccess(false)
        setHotkeyMessage(
          status.error ||
            `全局热键 "${status.currentHotkey || s.hotkey}" 注册失败。建议使用备用快捷键：${status.recommendedHotkey || 'Ctrl+Shift+Space'}`
        )
      }

      const tagList = await suijian.tags.list()
      setTags(tagList)
    } catch (err) {
      console.error('Failed to load settings or tags:', err)
      setLoadError('加载设置失败：' + toErrMsg(err))
    }
  }

  // 消息定时器统一由 ref 管理：set 前清旧值，卸载时清理，避免卸载后 setState
  const flashActionMsg = (msg: string) => {
    if (msgTimerRef.current) clearTimeout(msgTimerRef.current)
    setActionShortcutMsg(msg)
    msgTimerRef.current = setTimeout(() => setActionShortcutMsg(null), 3000)
  }

  useEffect(() => {
    return () => {
      if (msgTimerRef.current) clearTimeout(msgTimerRef.current)
    }
  }, [])

  useEffect(() => {
    loadSettingsAndTags()
  }, [])

  const SHORTCUT_FIELD_LABELS: Record<ShortcutField, string> = {
    hotkey: '全局召唤键',
    newNote: '新建便签',
    back: '返回搜索',
    dismiss: '隐藏/收起窗口'
  }

  const handleShortcutKeyDown = (
    e: React.KeyboardEvent<HTMLInputElement>,
    setter: (val: string) => void,
    field: ShortcutField
  ) => {
    if (e.key === 'Tab') return
    e.preventDefault()
    e.stopPropagation()
    e.nativeEvent?.stopImmediatePropagation?.()

    if (e.key === 'Backspace' || e.key === 'Delete') {
      setter('')
      setScWarnings((prev) => ({ ...prev, [field]: null }))
      return
    }
    if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) {
      return
    }
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
    const combo = parts.join('+')
    setter(combo)

    // 即时软冲突检测：与其他三项（含当前输入值）互斥 + 编辑器/系统键位；
    // 全局召唤键不检编辑器表（系统级 RegisterHotKey 命中时按键不进页面，让位文案无意义）
    const rawValues: Record<ShortcutField, string> = {
      hotkey: hotkeyInput,
      newNote: newNoteInput,
      back: backSearchInput,
      dismiss: dismissInput
    }
    const others = (Object.keys(rawValues) as ShortcutField[])
      .filter((k) => k !== field)
      .map((k) => ({ label: SHORTCUT_FIELD_LABELS[k], combo: normalizeShortcutSetting(rawValues[k]) }))
      .filter((o) => o.combo !== '')
    const warning = detectConflict(normalizeShortcutSetting(combo), others, field !== 'hotkey')
    setScWarnings((prev) => ({ ...prev, [field]: warning ? warning.message : null }))
  }

  const handleRegisterHotkey = async () => {
    if (!hotkeyInput.trim()) return
    try {
      const res = await suijian.settings.registerHotkey(hotkeyInput.trim())
      if (res.registered) {
        setHotkeySuccess(true)
        setHotkeyMessage(`全局热键 "${res.currentHotkey}" 注册成功`)
        const all = await suijian.settings.getAll()
        onSettingsChanged?.(all)
      } else {
        setHotkeySuccess(false)
        const rec = res.recommendedHotkey || 'Ctrl+Shift+Space'
        const errMsg = res.error || `热键 "${hotkeyInput.trim()}" 冲突或不可用`
        setHotkeyMessage(`${errMsg}。建议使用备用快捷键：${rec}`)
      }
    } catch (err) {
      setHotkeySuccess(false)
      setHotkeyMessage(`热键配置失败: ${toErrMsg(err)}`)
    }
  }

  const handleSaveActionShortcuts = async () => {
    try {
      const updated = await suijian.settings.updateActionShortcuts({
        shortcutNewNote: newNoteInput,
        shortcutBackToSearch: backSearchInput,
        shortcutDismiss: dismissInput
      })
      flashActionMsg('应用内快捷键已更新！')
      setScWarnings({})
      onSettingsChanged?.(updated)
    } catch (err) {
      setActionShortcutMsg(`更新失败: ${toErrMsg(err)}`)
      if (msgTimerRef.current) clearTimeout(msgTimerRef.current)
      msgTimerRef.current = setTimeout(() => setActionShortcutMsg(null), 5000)
    }
  }

  const handleRestoreDefaultShortcuts = async () => {
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
      flashActionMsg('快捷键已恢复默认设置！')
      onSettingsChanged?.(all)
    } catch (err) {
      setActionShortcutMsg(`恢复失败: ${toErrMsg(err)}`)
      if (msgTimerRef.current) clearTimeout(msgTimerRef.current)
      msgTimerRef.current = setTimeout(() => setActionShortcutMsg(null), 5000)
    }
  }

  // Tags
  const refreshTagsList = async () => {
    try {
      const tagList = await suijian.tags.list()
      setTags(tagList)
    } catch {
      // Ignore
    }
  }

  const handleCreateTag = async () => {
    const trimmed = newTagName.trim()
    if (!trimmed) return
    if (trimmed.length > 50) {
      setTagError('标签名称最多50个字符')
      return
    }
    setTagError(null)
    try {
      await suijian.tags.create({ name: trimmed })
      setNewTagName('')
      await refreshTagsList()
      onTagsChanged?.()
    } catch (err) {
      setTagError(`创建标签失败: ${toErrMsg(err)}`)
    }
  }

  const handleSaveRenameTag = async (id: string) => {
    const trimmed = editingTagName.trim()
    if (!trimmed) return
    if (trimmed.length > 50) {
      setTagError('标签名称最多50个字符')
      return
    }
    setTagError(null)
    try {
      await suijian.tags.rename({ id, name: trimmed })
      setEditingTagId(null)
      setEditingTagName('')
      await refreshTagsList()
      onTagsChanged?.()
    } catch (err) {
      setTagError(`重命名标签失败: ${toErrMsg(err)}`)
    }
  }

  const handleDeleteTag = async (id: string) => {
    try {
      await suijian.tags.delete(id)
      await refreshTagsList()
      onTagsChanged?.(id)
    } catch (err) {
      setTagError(`删除标签失败: ${toErrMsg(err)}`)
    }
  }

  // Preferences
  const handleToggleLaunchAtLogin = async (checked: boolean) => {
    setLaunchAtLogin(checked)
    try {
      const updated = await suijian.settings.update('launchAtLogin', checked)
      setPrefError(null)
      onSettingsChanged?.(updated)
    } catch (err) {
      console.error(err)
      setLaunchAtLogin(!checked) // 乐观开关失败后回滚，避免 UI 与实际状态不一致
      setPrefError('保存开机自启动设置失败：' + toErrMsg(err))
    }
  }

  const handleToggleAutoHideOnBlur = async (checked: boolean) => {
    setAutoHideOnBlur(checked)
    try {
      const updated = await suijian.settings.update('autoHideOnBlur', checked)
      setPrefError(null)
      onSettingsChanged?.(updated)
    } catch (err) {
      console.error(err)
      setAutoHideOnBlur(!checked) // 同上：失败回滚并提示
      setPrefError('保存自动收起设置失败：' + toErrMsg(err))
    }
  }

  // Backup
  const handleExportBackup = async () => {
    if (backupLoading) return
    setBackupLoading(true)
    setBackupMessage(null)
    try {
      const now = new Date()
      const filename = `suijian-backup-${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, '0')}${String(now.getDate()).padStart(2, '0')}.zip`
      const res = await suijian.backup.export(filename)
      if (res.canceled) {
        setBackupStatus('idle')
      } else {
        setBackupStatus('success')
        setBackupMessage(
          `备份已成功导出（共 ${res.noteCount ?? 0} 条便签，${res.attachmentCount ?? 0} 个附件）`
        )
      }
    } catch (err) {
      setBackupStatus('error')
      setBackupMessage(`导出备份失败: ${toErrMsg(err)}`)
    } finally {
      setBackupLoading(false)
    }
  }

  const handleInspectBackup = async () => {
    if (backupLoading) return
    setBackupLoading(true)
    setBackupMessage(null)
    try {
      const res = await suijian.backup.inspectSelect()
      if (!res.canceled) {
        setInspectResult(res)
      }
    } catch (err) {
      setBackupStatus('error')
      setBackupMessage(`选择备份包失败: ${toErrMsg(err)}`)
    } finally {
      setBackupLoading(false)
    }
  }

  const handleConfirmRestore = async () => {
    if (!inspectResult?.token) return
    setIsRestoring(true)
    setBackupMessage('正在检查并恢复数据，请稍候...')
    try {
      if (onBeforeRestore) {
        const canRestore = await onBeforeRestore()
        if (!canRestore) {
          setBackupStatus('error')
          setBackupMessage('当前正在编辑的便签保存未完成或失败，已取消恢复以避免丢失未保存内容')
          setIsRestoring(false)
          return
        }
      }
      const res = await suijian.backup.restoreConfirm(inspectResult.token)
      setBackupStatus('success')
      setBackupMessage(
        `资料库恢复成功！已恢复 ${res.restoredNoteCount ?? 0} 篇便签，${res.restoredTagCount ?? 0} 个标签，${res.restoredAttachmentCount ?? 0} 个附件。`
      )
      setInspectResult(null)
      onDataRestored?.()
    } catch (err) {
      setBackupStatus('error')
      setBackupMessage(`恢复备份失败: ${toErrMsg(err)}`)
    } finally {
      setIsRestoring(false)
    }
  }

  return (
    <div className="settings-view">
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '4px' }}>
        <h2 style={{ fontSize: '16px', fontWeight: 700 }}>设置</h2>
      </div>

      {/* 全局呼出热键 */}
      <div className="settings-section">
        <h3 className="settings-title">全局呼出热键</h3>
        <div className="setting-row">
          <div>
            <div>快捷键设置</div>
            <div className="setting-desc">按键组合，例如 Ctrl+Space 或 Ctrl+Shift+Space</div>
          </div>
          <div className="input-with-button" style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
            <input
              type="text"
              className="btn"
              style={{ width: '160px', textAlign: 'center', fontFamily: 'var(--font-mono)' }}
              value={hotkeyInput}
              onChange={(e) => setHotkeyInput(e.target.value)}
              onKeyDown={(e) => handleShortcutKeyDown(e, setHotkeyInput, 'hotkey')}
              placeholder="Ctrl+Space"
              aria-label="热键输入"
            />
            <button
              type="button"
              className="btn btn-primary"
              style={{ whiteSpace: 'nowrap', flexShrink: 0, minWidth: '60px' }}
              onClick={handleRegisterHotkey}
            >
              应用
            </button>
          </div>
        </div>
        {scWarnings.hotkey && (
          <div className="setting-desc" style={{ fontSize: '12px', color: '#b45309', marginTop: '4px' }}>
            {scWarnings.hotkey}
          </div>
        )}
        {hotkeyMessage && (
          <div
            style={{
              fontSize: '12px',
              marginTop: '8px',
              color: hotkeySuccess ? 'var(--sage-success)' : 'var(--danger-color)'
            }}
            role="status"
            aria-live="polite"
          >
            {hotkeyMessage}
          </div>
        )}
      </div>

      {/* 应用内动作快捷键 */}
      <div className="settings-section">
        <h3 className="settings-title">应用内快捷键</h3>
        <div className="setting-desc" style={{ marginBottom: '12px' }}>
          支持单键（如 Esc）或组合键（如 Ctrl+N、Ctrl+E）。点击输入框后直接按下物理键位即可录入。
        </div>

        <div className="setting-row">
          <div>
            <div>新建便签快捷键</div>
            <div className="setting-desc">在任意界面快速新建空白便签</div>
          </div>
          <input
            type="text"
            className="btn"
            style={{ width: '140px', textAlign: 'center', fontFamily: 'var(--font-mono)' }}
            value={newNoteInput}
            onChange={(e) => setNewNoteInput(e.target.value)}
            onKeyDown={(e) => handleShortcutKeyDown(e, setNewNoteInput, 'newNote')}
            placeholder="Ctrl+N"
            aria-label="新建便签快捷键"
          />
        </div>
        {scWarnings.newNote && (
          <div className="setting-desc" style={{ fontSize: '12px', color: '#b45309', marginTop: '-4px', marginBottom: '8px' }}>
            {scWarnings.newNote}
          </div>
        )}

        <div className="setting-row">
          <div>
            <div>返回搜索列表快捷键</div>
            <div className="setting-desc">在编辑器中快速保存并返回搜索主界面</div>
          </div>
          <input
            type="text"
            className="btn"
            style={{ width: '140px', textAlign: 'center', fontFamily: 'var(--font-mono)' }}
            value={backSearchInput}
            onChange={(e) => setBackSearchInput(e.target.value)}
            onKeyDown={(e) => handleShortcutKeyDown(e, setBackSearchInput, 'back')}
            placeholder="Ctrl+E"
            aria-label="返回搜索列表快捷键"
          />
        </div>
        {scWarnings.back && (
          <div className="setting-desc" style={{ fontSize: '12px', color: '#b45309', marginTop: '-4px', marginBottom: '8px' }}>
            {scWarnings.back}
          </div>
        )}

        <div className="setting-row">
          <div>
            <div>隐藏/收起窗口快捷键</div>
            <div className="setting-desc">关闭面板并驻留系统托盘（默认 Escape）</div>
          </div>
          <input
            type="text"
            className="btn"
            style={{ width: '140px', textAlign: 'center', fontFamily: 'var(--font-mono)' }}
            value={dismissInput}
            onChange={(e) => setDismissInput(e.target.value)}
            onKeyDown={(e) => handleShortcutKeyDown(e, setDismissInput, 'dismiss')}
            placeholder="Escape"
            aria-label="隐藏窗口快捷键"
          />
        </div>
        {scWarnings.dismiss && (
          <div className="setting-desc" style={{ fontSize: '12px', color: '#b45309', marginTop: '-4px', marginBottom: '8px' }}>
            {scWarnings.dismiss}
          </div>
        )}

        <div style={{ display: 'flex', gap: '8px', marginTop: '12px', alignItems: 'center' }}>
          <button type="button" className="btn btn-primary" onClick={handleSaveActionShortcuts}>
            保存按键设置
          </button>
          <button type="button" className="btn" onClick={handleRestoreDefaultShortcuts}>
            恢复默认按键
          </button>
          {actionShortcutMsg && (
            <span style={{ fontSize: '12px', color: 'var(--sage-success)' }}>{actionShortcutMsg}</span>
          )}
        </div>
      </div>

      {/* 标签管理 */}
      <div className="settings-section">
        <h3 className="settings-title">标签管理</h3>
        <div className="setting-desc" style={{ marginBottom: '12px' }}>
          创建、重命名或删除便签分类标签。删除标签不会影响便签内容本身。
        </div>

        {/* 新建标签输入行 */}
        <div style={{ display: 'flex', gap: '8px', marginBottom: '12px' }}>
          <input
            type="text"
            className="btn"
            style={{ flex: 1, textAlign: 'left', fontFamily: 'var(--font-sans)', height: '32px' }}
            value={newTagName}
            onChange={(e) => setNewTagName(e.target.value)}
            placeholder="输入新标签名称..."
            maxLength={50}
            aria-label="新标签名称输入框"
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                handleCreateTag()
              }
            }}
          />
          <button
            type="button"
            className="btn btn-primary"
            style={{ height: '32px' }}
            onClick={handleCreateTag}
            aria-label="创建新标签"
          >
            创建标签
          </button>
        </div>

        {tagError && (
          <div style={{ fontSize: '12px', color: 'var(--danger-color)', marginBottom: '8px' }}>
            {tagError}
          </div>
        )}

        {/* 标签列表 */}
        <div className="tags-management-list" role="list" aria-label="已建标签列表">
          {tags.length === 0 ? (
            <div style={{ fontSize: '12px', color: 'var(--text-secondary)', padding: '6px 0' }}>
              暂无标签
            </div>
          ) : (
            tags.map((tag) => (
              <div key={tag.id} className="tag-management-item" role="listitem">
                {editingTagId === tag.id ? (
                  <div style={{ display: 'flex', gap: '8px', flex: 1, marginRight: '8px' }}>
                    <input
                      type="text"
                      className="btn"
                      style={{ flex: 1, textAlign: 'left', fontFamily: 'var(--font-sans)' }}
                      value={editingTagName}
                      onChange={(e) => setEditingTagName(e.target.value)}
                      maxLength={50}
                      autoFocus
                      aria-label={`重命名标签 ${tag.name}`}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') {
                          e.preventDefault()
                          handleSaveRenameTag(tag.id)
                        } else if (e.key === 'Escape') {
                          e.preventDefault()
                          setEditingTagId(null)
                        }
                      }}
                    />
                    <button
                      type="button"
                      className="btn btn-primary"
                      style={{ padding: '2px 8px', fontSize: '12px' }}
                      onClick={() => handleSaveRenameTag(tag.id)}
                      aria-label="保存重命名"
                    >
                      保存
                    </button>
                    <button
                      type="button"
                      className="btn"
                      style={{ padding: '2px 8px', fontSize: '12px' }}
                      onClick={() => setEditingTagId(null)}
                      aria-label="取消重命名"
                    >
                      取消
                    </button>
                  </div>
                ) : (
                  <span style={{ fontWeight: 500, color: tag.color || 'inherit' }}>#{tag.name}</span>
                )}

                {editingTagId !== tag.id && (
                  <div style={{ display: 'flex', gap: '8px' }}>
                    <button
                      type="button"
                      className="btn"
                      style={{ padding: '2px 8px', fontSize: '12px' }}
                      onClick={() => {
                        setEditingTagId(tag.id)
                        setEditingTagName(tag.name)
                      }}
                      aria-label={`重命名 ${tag.name}`}
                    >
                      重命名
                    </button>
                    <button
                      type="button"
                      className="btn"
                      style={{ padding: '2px 8px', fontSize: '12px', color: 'var(--danger-color)' }}
                      onClick={() => handleDeleteTag(tag.id)}
                      aria-label={`删除标签 ${tag.name}`}
                    >
                      删除
                    </button>
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </div>

      {/* 系统偏好 */}
      <div className="settings-section">
        <h3 className="settings-title">系统偏好</h3>
        {loadError && <div className="error-banner">{loadError}</div>}
        {prefError && <div className="error-banner">{prefError}</div>}
        <div className="setting-row">
          <div>
            <div>开机自启动</div>
            <div className="setting-desc">Windows 登录时在后台静默启动随笺</div>
          </div>
          <input
            type="checkbox"
            checked={launchAtLogin}
            onChange={(e) => handleToggleLaunchAtLogin(e.target.checked)}
            aria-label="开机自启动开关"
          />
        </div>

        <div className="setting-row">
          <div>
            <div>失去焦点自动收起</div>
            <div className="setting-desc">当面板失去系统焦点时自动隐藏窗口</div>
          </div>
          <input
            type="checkbox"
            checked={autoHideOnBlur}
            onChange={(e) => handleToggleAutoHideOnBlur(e.target.checked)}
            aria-label="失去焦点自动收起开关"
          />
        </div>
      </div>

      {/* 数据备份与安全恢复 */}
      <div className="settings-section">
        <h3 className="settings-title">数据备份与安全恢复</h3>
        <div className="setting-desc" style={{ marginBottom: '12px' }}>
          导出包含完整便签、标签、图片附件及离线 HTML 的 ZIP 备份包；或从备份包中安全恢复资料库。
        </div>

        <div className="setting-row" style={{ alignItems: 'center' }}>
          <div>
            <div>导出备份</div>
            <div className="setting-desc">将当前所有便签、标签与图片附件导出为 ZIP 文件</div>
          </div>
          <button
            type="button"
            className="btn btn-primary"
            onClick={handleExportBackup}
            disabled={backupLoading}
            aria-label="导出备份"
          >
            {backupLoading ? '处理中...' : '导出备份'}
          </button>
        </div>

        <div className="setting-row" style={{ alignItems: 'center', marginTop: '12px' }}>
          <div>
            <div>恢复备份</div>
            <div className="setting-desc">从备份 ZIP 文件安全恢复并替换当前本地资料库</div>
          </div>
          <button
            type="button"
            className="btn"
            style={{ borderColor: 'var(--cobalt-focus)', color: 'var(--cobalt-focus)' }}
            onClick={handleInspectBackup}
            disabled={backupLoading || isRestoring}
            aria-label="恢复备份"
          >
            {backupLoading ? '处理中...' : '恢复备份'}
          </button>
        </div>

        {inspectResult && (
          <div
            style={{
              marginTop: '12px',
              padding: '12px',
              borderRadius: '6px',
              border: '1px solid var(--border-color)',
              background: 'var(--hover-bg)'
            }}
          >
            <div style={{ fontWeight: 600, fontSize: '13px', marginBottom: '6px' }}>
              备份包解析确认
            </div>
            <div style={{ fontSize: '12px', color: 'var(--text-secondary)', marginBottom: '8px' }}>
              包含 {inspectResult.noteCount} 篇便签，{inspectResult.tagCount} 个标签，{inspectResult.attachmentCount} 个附件。
            </div>
            <div style={{ display: 'flex', gap: '8px' }}>
              <button
                type="button"
                className="btn btn-primary"
                onClick={handleConfirmRestore}
                disabled={isRestoring}
              >
                {isRestoring ? '恢复中...' : '确认导入并恢复'}
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => setInspectResult(null)}
                disabled={isRestoring}
              >
                取消
              </button>
            </div>
          </div>
        )}

        {backupMessage && (
          <div
            style={{
              fontSize: '12px',
              marginTop: '10px',
              padding: '8px 12px',
              borderRadius: '4px',
              backgroundColor:
                backupStatus === 'error' ? 'rgba(239, 68, 68, 0.1)' : 'rgba(16, 185, 129, 0.1)',
              color: backupStatus === 'error' ? 'var(--danger-color)' : 'var(--sage-success)'
            }}
            role="status"
          >
            {backupMessage}
          </div>
        )}
      </div>
    </div>
  )
}
