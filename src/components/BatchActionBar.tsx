import React from 'react'
import type { NoteScope } from '../types'
import { UiIcon } from './UiIcon'

interface BatchActionBarProps {
  selectedCount: number
  totalCount: number
  allSelected: boolean
  scope: NoteScope
  onToggleSelectAll: () => void
  onClearSelection: () => void
  onBatchTrash: () => void
  onBatchDeletePermanently: () => void
  onEmptyTrash: () => void
}

export const BatchActionBar: React.FC<BatchActionBarProps> = ({
  selectedCount,
  totalCount,
  allSelected,
  scope,
  onToggleSelectAll,
  onClearSelection,
  onBatchTrash,
  onBatchDeletePermanently,
  onEmptyTrash
}) => {
  if (selectedCount === 0 && scope !== 'trash') {
    return null
  }

  return (
    <div className="batch-action-bar">
      <div className="batch-left">
        <label className="batch-select-all-label">
          <input
            type="checkbox"
            checked={allSelected && totalCount > 0}
            onChange={onToggleSelectAll}
            className="batch-checkbox"
          />
          <span>全选当前 ({totalCount})</span>
        </label>

        {selectedCount > 0 && (
          <>
            <span className="batch-divider">|</span>
            <span className="batch-count-info">已选 {selectedCount} 项</span>
            <button className="batch-text-btn" onClick={onClearSelection}>
              取消选择
            </button>
          </>
        )}
      </div>

      <div className="batch-right">
        {selectedCount > 0 && (scope === 'active' || scope === 'archived') && (
          <button className="batch-action-btn danger-btn" onClick={onBatchTrash}>
            <UiIcon name="trash" size={14} /> 移入回收站 ({selectedCount})
          </button>
        )}

        {selectedCount > 0 && scope === 'trash' && (
          <button className="batch-action-btn danger-btn" onClick={onBatchDeletePermanently}>
            <UiIcon name="warning" size={14} /> 彻底删除所选 ({selectedCount})
          </button>
        )}

        {scope === 'trash' && totalCount > 0 && (
          <button className="batch-action-btn empty-trash-btn" onClick={onEmptyTrash}>
            清空回收站
          </button>
        )}
      </div>
    </div>
  )
}
