import React from 'react'
import { UiIcon } from './UiIcon'
import type { Note } from '../types'

interface NoteListProps {
  notes: Note[]
  focusedIndex: number
  selectedIds: Set<string>
  onSelectNote: (note: Note) => void
  onToggleCheck: (id: string) => void
  onFocusIndex: (index: number) => void
  searchQuery?: string
  // 新建快捷键展示值；undefined 表示当前场景不可行动（回收站/标签筛选/只读恢复），不渲染新建提示
  newNoteShortcut?: string
}

function formatDate(isoStr: string): string {
  try {
    const d = new Date(isoStr)
    const t = d.getTime()
    if (Number.isNaN(t)) return ''
    const now = new Date()
    const diffMs = now.getTime() - t
    const diffMins = Math.floor(diffMs / 60000)
    const diffHours = Math.floor(diffMins / 60)
    const diffDays = Math.floor(diffHours / 24)

    if (diffMins < 1) return '刚刚'
    if (diffMins < 60) return `${diffMins} 分钟前`
    if (diffHours < 24) return `${diffHours} 小时前`
    if (diffDays === 1) return '昨天'
    if (diffDays < 7) return `${diffDays} 天前`

    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  } catch {
    return ''
  }
}

export const NoteList: React.FC<NoteListProps> = ({
  notes,
  focusedIndex,
  selectedIds,
  onSelectNote,
  onToggleCheck,
  onFocusIndex,
  searchQuery,
  newNoteShortcut
}) => {
  if (notes.length === 0) {
    return (
      <div className="notes-list-scroll empty-container">
        <div className="empty-state">
          <p>未找到符合条件的便签</p>
          {searchQuery && searchQuery.trim() ? (
            <p className="empty-create-hint">
              按 Enter 键以 &ldquo;{searchQuery.trim()}&rdquo; 为标题新建便签
            </p>
          ) : (
            newNoteShortcut && (
              <p className="empty-create-hint">按 {newNoteShortcut} 创建新便签</p>
            )
          )}
        </div>
      </div>
    )
  }

  const pinnedNotes = notes.filter((n) => n.is_pinned && !n.archived_at && !n.deleted_at)
  const recentNotes = notes.filter((n) => !n.is_pinned || n.archived_at || n.deleted_at)

  // focusedIndex 语义基于完整 notes 数组（App 层上下键直接索引），置顶/最近子数组的位置索引不可用；
  // 渲染前建一次 id→全量索引的 Map，消除 renderNoteCard 内 indexOf 的 O(n²)
  const indexById = new Map(notes.map((n, i) => [n.id, i] as const))

  const renderNoteCard = (note: Note) => {
    const originalIndex = indexById.get(note.id) ?? 0
    const isFocused = originalIndex === focusedIndex
    const isChecked = selectedIds.has(note.id)

    return (
      <div
        key={note.id}
        role="option"
        aria-selected={isFocused}
        className={`note-item ${isFocused ? 'selected' : ''}`}
        onClick={() => onSelectNote(note)}
        onMouseEnter={() => onFocusIndex(originalIndex)}
      >
        <div className="note-item-header">
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px', minWidth: 0 }}>
            <input
              type="checkbox"
              checked={isChecked}
              onChange={(e) => {
                e.stopPropagation()
                onToggleCheck(note.id)
              }}
              onClick={(e) => e.stopPropagation()}
              className="note-checkbox"
              aria-label={`选择便签 ${note.title || '未命名便签'}`}
            />
            <span className="note-item-title">{note.title || '未命名便签'}</span>
          </div>

          <div className="note-item-meta">
            {note.is_pinned && (
              <span className="pin-icon" role="img" title="置顶" aria-label="置顶">
                <UiIcon name="pin" size={13} />
              </span>
            )}
            <span>{formatDate(note.updated_at)}</span>
          </div>
        </div>

        {note.plain_text && <p className="note-item-preview">{note.plain_text}</p>}

        {note.tags && note.tags.length > 0 && (
          <div className="note-item-tags">
            {note.tags.map((t) => (
              <span key={t.id} className="tag-badge">
                #{t.name}
              </span>
            ))}
          </div>
        )}
      </div>
    )
  }

  return (
    <div className="notes-list-scroll" role="listbox" aria-label="便签列表">
      {pinnedNotes.length > 0 && (
        <div className="pinned-section">
          <div className="section-header">已置顶</div>
          {pinnedNotes.map(renderNoteCard)}
        </div>
      )}

      {recentNotes.length > 0 && (
        <div className="recent-section">
          {pinnedNotes.length > 0 && <div className="section-header">最近便签</div>}
          {recentNotes.map(renderNoteCard)}
        </div>
      )}
    </div>
  )
}
