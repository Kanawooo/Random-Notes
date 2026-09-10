import React, { useRef, useEffect } from 'react'
import type { NoteScope, Tag } from '../types'

interface SearchBarProps {
  query: string
  onQueryChange: (q: string) => void
  scope: NoteScope
  onScopeChange: (scope: NoteScope) => void
  tags?: Tag[]
  selectedTagId?: string | null
  onSelectTag?: (tagId: string | null) => void
  totalCount: number
  newNoteShortcut?: string
  focusTrigger?: number
}

export const SearchBar: React.FC<SearchBarProps> = ({
  query,
  onQueryChange,
  scope,
  onScopeChange,
  tags = [],
  selectedTagId = null,
  onSelectTag,
  totalCount,
  newNoteShortcut = 'Ctrl+N',
  focusTrigger
}) => {
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus()
    inputRef.current?.select()
  }, [focusTrigger])

  return (
    <div className="search-bar-container">
      {/* 搜索框 */}
      <div className="search-input-wrapper">
        <input
          ref={inputRef}
          type="text"
          className="search-input"
          placeholder={`搜索便签内容或标签... (Enter 打开，${newNoteShortcut} 新建)`}
          value={query}
          onChange={(e) => onQueryChange(e.target.value)}
          autoFocus
          aria-label="便签全局搜索框"
        />
      </div>

      {/* 范围与标签筛选 */}
      <div className="filter-bar">
        <div className="scope-pills" role="group" aria-label="便签范围筛选">
          <button
            type="button"
            className={`pill ${scope === 'active' ? 'active' : ''}`}
            onClick={() => onScopeChange('active')}
            aria-label="常规便签"
            aria-pressed={scope === 'active'}
          >
            便签 {scope === 'active' && totalCount > 0 ? `(${totalCount})` : ''}
          </button>
          <button
            type="button"
            className={`pill ${scope === 'archived' ? 'active' : ''}`}
            onClick={() => onScopeChange('archived')}
            aria-label="归档便签"
            aria-pressed={scope === 'archived'}
          >
            归档 {scope === 'archived' && totalCount > 0 ? `(${totalCount})` : ''}
          </button>
          <button
            type="button"
            className={`pill ${scope === 'trash' ? 'active' : ''}`}
            onClick={() => onScopeChange('trash')}
            aria-label="回收站"
            aria-pressed={scope === 'trash'}
          >
            回收站 {scope === 'trash' && totalCount > 0 ? `(${totalCount})` : ''}
          </button>
        </div>

        {tags && tags.length > 0 && onSelectTag && (
          <div className="tag-pills" role="group" aria-label="标签筛选">
            <button
              type="button"
              className={`tag-pill ${selectedTagId === null ? 'active' : ''}`}
              onClick={() => onSelectTag(null)}
              aria-pressed={selectedTagId === null}
            >
              全部
            </button>
            {tags.map((t) => (
              <button
                key={t.id}
                type="button"
                className={`tag-pill ${selectedTagId === t.id ? 'active' : ''}`}
                onClick={() => onSelectTag(selectedTagId === t.id ? null : t.id)}
                aria-pressed={selectedTagId === t.id}
              >
                #{t.name}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
