import React, { useState } from 'react'
import { suijian } from '../lib/api'
import { toErrMsg } from '../lib/errors'
import type { Tag } from '../types'
import { UiIcon } from './UiIcon'

interface TagModalProps {
  isOpen: boolean
  tags: Tag[]
  onClose: () => void
  onRefreshTags: () => void
}

const PRESET_COLORS = ['#4D6F9F', '#667F6B', '#D97706', '#DC2626', '#7C3AED', '#DB2777', '#2563EB', '#4B5563']

export const TagModal: React.FC<TagModalProps> = ({
  isOpen,
  tags,
  onClose,
  onRefreshTags
}) => {
  const [newTagName, setNewTagName] = useState('')
  const [newTagColor, setNewTagColor] = useState(PRESET_COLORS[0])
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editingName, setEditingName] = useState('')
  const [errorMsg, setErrorMsg] = useState<string | null>(null)

  if (!isOpen) return null

  const handleCreateTag = async () => {
    if (!newTagName.trim()) return
    setErrorMsg(null)
    try {
      await suijian.tags.create({
        name: newTagName.trim(),
        color: newTagColor
      })
      setNewTagName('')
      onRefreshTags()
    } catch (err: unknown) {
      setErrorMsg(toErrMsg(err))
    }
  }

  const handleSaveRename = async (id: string) => {
    if (!editingName.trim()) return
    setErrorMsg(null)
    try {
      await suijian.tags.rename({
        id,
        name: editingName.trim()
      })
      setEditingId(null)
      onRefreshTags()
    } catch (err: unknown) {
      setErrorMsg(toErrMsg(err))
    }
  }

  const handleDeleteTag = async (id: string) => {
    setErrorMsg(null)
    try {
      await suijian.tags.delete(id)
      onRefreshTags()
    } catch (err: unknown) {
      setErrorMsg(toErrMsg(err))
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal-card" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h3>管理标签</h3>
          <button
            className="modal-close-btn"
            onClick={onClose}
            title="关闭"
            aria-label="关闭"
          >
            <UiIcon name="close" size={16} />
          </button>
        </div>

        {errorMsg && <div className="error-banner">{errorMsg}</div>}

        <div className="modal-body tag-modal-body">
          {/* Create Row */}
          <div className="tag-create-row">
            <input
              type="text"
              className="tag-create-input"
              value={newTagName}
              onChange={(e) => setNewTagName(e.target.value)}
              placeholder="新标签名称..."
              onKeyDown={(e) => e.key === 'Enter' && handleCreateTag()}
            />
            <div className="tag-color-picker">
              {PRESET_COLORS.map((c) => (
                <button
                  key={c}
                  type="button"
                  className={`color-dot-btn ${newTagColor === c ? 'active' : ''}`}
                  style={{ backgroundColor: c }}
                  onClick={() => setNewTagColor(c)}
                />
              ))}
            </div>
            <button className="action-btn" onClick={handleCreateTag}>
              添加
            </button>
          </div>

          {/* Tag List */}
          <div className="tag-manage-list">
            {tags.map((t) => (
              <div key={t.id} className="tag-manage-item">
                <span className="tag-color-indicator" style={{ backgroundColor: t.color }} />
                {editingId === t.id ? (
                  <input
                    type="text"
                    className="tag-edit-input"
                    value={editingName}
                    onChange={(e) => setEditingName(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && handleSaveRename(t.id)}
                    autoFocus
                  />
                ) : (
                  <span className="tag-name-text">{t.name}</span>
                )}

                <div className="tag-item-actions">
                  {editingId === t.id ? (
                    <button className="btn-small" onClick={() => handleSaveRename(t.id)}>
                      保存
                    </button>
                  ) : (
                    <button
                      className="btn-small"
                      onClick={() => {
                        setEditingId(t.id)
                        setEditingName(t.name)
                      }}
                    >
                      重命名
                    </button>
                  )}
                  <button className="btn-small danger-text" onClick={() => handleDeleteTag(t.id)}>
                    删除
                  </button>
                </div>
              </div>
            ))}
            {tags.length === 0 && <div className="tag-empty-hint">暂无标签，在上方输入创建。</div>}
          </div>
        </div>
      </div>
    </div>
  )
}
