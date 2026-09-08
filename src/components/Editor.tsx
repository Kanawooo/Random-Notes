import React, { useState, useEffect, useRef, useCallback } from 'react'
import { useEditor, EditorContent } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import Placeholder from '@tiptap/extension-placeholder'
import Image from '@tiptap/extension-image'
import Link from '@tiptap/extension-link'
import TaskList from '@tiptap/extension-task-list'
import TaskItem from '@tiptap/extension-task-item'
import { suijian } from '../lib/api'
import { sanitizePastedHtml } from '../lib/sanitize'
import { useAutoSave } from '../hooks/useAutoSave'
import { UiIcon } from './UiIcon'
import type { Note, Tag } from '../types'

interface EditorProps {
  note: Note
  tags: Tag[]
  onBack: () => void
  onNoteUpdated: (note: Note) => void
  onOpenTagModal: () => void
  onRegisterFlush?: (flush: () => Promise<boolean>) => () => void
  backShortcutText?: string
}

export const Editor: React.FC<EditorProps> = ({
  note,
  tags,
  onBack,
  onNoteUpdated,
  onOpenTagModal,
  onRegisterFlush
}) => {
  const [title, setTitle] = useState(note.title)
  const [titleManuallyEdited, setTitleManuallyEdited] = useState(note.title_manually_edited)
  const [isPinned, setIsPinned] = useState(note.is_pinned)
  const [errorMsg, setErrorMsg] = useState<string | null>(null)

  const noteRef = useRef(note)
  const titleRef = useRef(title)
  const titleManuallyEditedRef = useRef(titleManuallyEdited)
  const isPinnedRef = useRef(isPinned)

  useEffect(() => {
    noteRef.current = note
    setTitle(note.title)
    setTitleManuallyEdited(note.title_manually_edited)
    setIsPinned(note.is_pinned)
  }, [note])

  useEffect(() => {
    titleRef.current = title
  }, [title])

  useEffect(() => {
    titleManuallyEditedRef.current = titleManuallyEdited
  }, [titleManuallyEdited])

  useEffect(() => {
    isPinnedRef.current = isPinned
  }, [isPinned])

  const { saveStatus, scheduleSave, flushSave, retryLastSave } = useAutoSave({
    note,
    onNoteUpdated: (updated) => {
      onNoteUpdated(updated)
      setIsPinned(updated.is_pinned)
    },
    onError: (err) => setErrorMsg(err)
  })

  useEffect(() => {
    if (onRegisterFlush) {
      return onRegisterFlush(flushSave)
    }
  }, [onRegisterFlush, flushSave])

  // Parse initial content JSON safely
  const initialContent = React.useMemo(() => {
    try {
      if (note.content_json && note.content_json !== '{}') {
        const parsed = JSON.parse(note.content_json)
        if (parsed && typeof parsed === 'object' && parsed.type === 'doc') {
          return parsed
        }
      }
    } catch {
      // fallback
    }
    return {
      type: 'doc',
      content: [{ type: 'paragraph', content: note.plain_text ? [{ type: 'text', text: note.plain_text }] : [] }]
    }
  }, [note.content_json, note.plain_text])

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
        codeBlock: {
          HTMLAttributes: {
            class: 'cascadia-code-block'
          }
        }
      }),
      Placeholder.configure({
        placeholder: '输入正文内容... 支持 Markdown 与图片粘贴'
      }),
      Image.configure({
        inline: false,
        allowBase64: false
      }),
      Link.configure({
        openOnClick: false,
        protocols: ['http', 'https', 'mailto']
      }),
      TaskList,
      TaskItem.configure({ nested: true })
    ],
    content: initialContent,
    onUpdate: ({ editor: ed }) => {
      const json = JSON.stringify(ed.getJSON())
      const plain = ed.getText()

      let newTitle = titleRef.current
      if (!titleManuallyEditedRef.current) {
        const firstLine = plain.split('\n').find((l) => l.trim().length > 0)
        if (firstLine) {
          newTitle = firstLine.trim().slice(0, 30)
          setTitle(newTitle)
        } else {
          newTitle = '未命名便签'
          setTitle(newTitle)
        }
      }

      scheduleSave({
        title: newTitle,
        content_json: json,
        plain_text: plain,
        title_manually_edited: titleManuallyEditedRef.current
      })
    },
    editorProps: {
      transformPastedHTML(html) {
        return sanitizePastedHtml(html)
      },
      handleClick(_view, _pos, event) {
        const target = (event.target as HTMLElement).closest('a')
        if (target && target.href) {
          event.preventDefault()
          try {
            const url = new URL(target.href)
            if (['http:', 'https:', 'mailto:'].includes(url.protocol)) {
              suijian.app.openExternal(target.href).catch((err) => {
                setErrorMsg(`打开外部链接失败: ${err}`)
              })
            } else {
              setErrorMsg('安全限制：仅允许打开 http, https 及 mailto 链接')
            }
          } catch {
            setErrorMsg('非法的外部链接地址')
          }
          return true
        }
        return false
      },
      handlePaste(_view, event) {
        const items = event.clipboardData?.items
        if (items) {
          for (let i = 0; i < items.length; i++) {
            if (items[i].type.startsWith('image/')) {
              event.preventDefault()
              const currentNoteId = noteRef.current.id
              suijian.attachments
                .addFromClipboard(currentNoteId)
                .then((att) => {
                  if (editor) {
                    editor
                      .chain()
                      .focus()
                      .setImage({ src: `suijian-attachment://${att.id}` })
                      .run()
                  }
                })
                .catch((err) => {
                  setErrorMsg(`粘贴图片失败: ${err instanceof Error ? err.message : String(err)}`)
                })
              return true
            }
          }
        }
        return false
      }
    }
  })

  const handlePasteImage = useCallback(async () => {
    try {
      const att = await suijian.attachments.addFromClipboard(noteRef.current.id)
      if (editor) {
        editor
          .chain()
          .focus()
          .setImage({ src: `suijian-attachment://${att.id}` })
          .run()
      }
    } catch (err) {
      setErrorMsg(`无法从剪贴板粘贴图片: ${err instanceof Error ? err.message : String(err)}`)
    }
  }, [editor])

  const handleSetLink = useCallback(() => {
    if (!editor) return
    const previousUrl = String(editor.getAttributes('link')['href'] || '')
    const url = window.prompt('请输入链接地址 (http, https, mailto):', previousUrl)
    if (url === null) return
    const trimmed = url.trim()
    if (trimmed === '') {
      editor.chain().focus().extendMarkRange('link').unsetLink().run()
      return
    }
    try {
      const parsed = new URL(trimmed)
      if (!['http:', 'https:', 'mailto:'].includes(parsed.protocol)) {
        setErrorMsg('安全限制：仅允许 http, https 或 mailto 协议')
        return
      }
      editor.chain().focus().extendMarkRange('link').setLink({ href: parsed.href }).run()
    } catch {
      setErrorMsg('请输入有效的 URL 地址')
    }
  }, [editor])

  const handleTitleChange = (newTitle: string) => {
    setTitle(newTitle)
    setTitleManuallyEdited(true)
    scheduleSave({
      title: newTitle,
      title_manually_edited: true
    })
  }

  const handleTogglePin = async () => {
    const ok = await flushSave()
    if (!ok) {
      setErrorMsg('便签正在保存或保存失败，无法更改置顶状态')
      return
    }
    const nextPin = !isPinned
    setIsPinned(nextPin)
    try {
      const updated = await suijian.notes.pin(note.id, nextPin)
      onNoteUpdated(updated)
    } catch (err) {
      setIsPinned(!nextPin)
      setErrorMsg(`更新置顶状态失败: ${err instanceof Error ? err.message : String(err)}`)
    }
  }

  const handleToggleArchive = async () => {
    const ok = await flushSave()
    if (!ok) {
      setErrorMsg('便签正在保存或保存失败，无法归档')
      return
    }
    try {
      if (note.archived_at) {
        const updated = await suijian.notes.unarchive(note.id)
        onNoteUpdated(updated)
      } else {
        const updated = await suijian.notes.archive(note.id)
        onNoteUpdated(updated)
      }
      onBack()
    } catch (err) {
      setErrorMsg(`归档操作失败: ${err instanceof Error ? err.message : String(err)}`)
    }
  }

  const handleTrash = async () => {
    const ok = await flushSave()
    if (!ok) {
      setErrorMsg('便签正在保存或保存失败，无法删除')
      return
    }
    try {
      const updated = await suijian.notes.trash(note.id)
      onNoteUpdated(updated)
      onBack()
    } catch (err) {
      setErrorMsg(`移入回收站失败: ${err instanceof Error ? err.message : String(err)}`)
    }
  }

  const handleRestore = async () => {
    try {
      const updated = await suijian.notes.restore(note.id)
      onNoteUpdated(updated)
    } catch (err) {
      setErrorMsg(`恢复便签失败: ${err instanceof Error ? err.message : String(err)}`)
    }
  }

  const handleAddTag = async (tagId: string) => {
    const ok = await flushSave()
    if (!ok) {
      setErrorMsg('便签正在保存或保存失败，无法添加标签')
      return
    }
    const currentTagIds = (note.tags || []).map((t) => t.id)
    if (currentTagIds.includes(tagId)) return
    const nextIds = [...currentTagIds, tagId]
    try {
      await suijian.tags.assign({ noteId: note.id, tagIds: nextIds })
      const refreshed = await suijian.notes.get(note.id)
      if (refreshed) onNoteUpdated(refreshed)
    } catch (err) {
      setErrorMsg(`添加标签失败: ${err instanceof Error ? err.message : String(err)}`)
    }
  }

  const handleRemoveTag = async (tagId: string) => {
    const ok = await flushSave()
    if (!ok) {
      setErrorMsg('便签正在保存或保存失败，无法移除标签')
      return
    }
    const currentTagIds = (note.tags || []).map((t) => t.id)
    const nextIds = currentTagIds.filter((id) => id !== tagId)
    try {
      await suijian.tags.assign({ noteId: note.id, tagIds: nextIds })
      const refreshed = await suijian.notes.get(note.id)
      if (refreshed) onNoteUpdated(refreshed)
    } catch (err) {
      setErrorMsg(`移除标签失败: ${err instanceof Error ? err.message : String(err)}`)
    }
  }

  return (
    <div className="editor-view">
      {/* 编辑器头部 */}
      <div className="editor-header">
        <div className="editor-header-left">
          <input
            type="text"
            className="note-title-input"
            value={title}
            onChange={(e) => handleTitleChange(e.target.value)}
            placeholder="便签标题..."
            aria-label="便签标题输入框"
          />
        </div>

        <div className="editor-header-right">
          {/* 保存状态指示器 */}
          <div className={`save-indicator ${saveStatus}`}>
            {saveStatus === 'saving' && (
              <>
                <UiIcon name="hourglass" size={13} style={{ marginRight: 4 }} />
                <span>保存中...</span>
              </>
            )}
            {saveStatus === 'saved' && '● 已保存'}
            {saveStatus === 'error' && (
              <span>
                <UiIcon name="warning" size={13} style={{ marginRight: 4 }} />
                保存失败
                <button
                  type="button"
                  className="btn"
                  style={{ padding: '1px 6px', fontSize: '11px', marginLeft: '6px' }}
                  onClick={() => retryLastSave()}
                  aria-label="重试保存"
                >
                  重试
                </button>
              </span>
            )}
          </div>

          {/* 置顶切换 */}
          <button
            type="button"
            className={`btn btn-icon ${isPinned ? 'btn-active' : ''}`}
            onClick={handleTogglePin}
            aria-label={isPinned ? '取消置顶' : '置顶便签'}
            title={isPinned ? '取消置顶' : '置顶便签'}
          >
            <UiIcon name="pin" size={15} />
          </button>

          {/* 归档 / 取消归档 */}
          {!note.deleted_at && (
            <button
              type="button"
              className="btn"
              onClick={handleToggleArchive}
              aria-label={note.archived_at ? '取消归档' : '归档便签'}
              title="归档便签"
            >
              {note.archived_at ? '取消归档' : '归档'}
            </button>
          )}

          {/* 回收站恢复 / 删除 */}
          {note.deleted_at ? (
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleRestore}
              aria-label="从回收站恢复"
            >
              恢复
            </button>
          ) : (
            <button
              type="button"
              className="btn btn-icon"
              onClick={handleTrash}
              aria-label="移入回收站"
              title="移入回收站"
            >
              <UiIcon name="trash" size={15} />
            </button>
          )}
        </div>
      </div>

      {errorMsg && (
        <div className="error-banner" role="alert">
          <span>{errorMsg}</span>
          <button
            type="button"
            className="btn btn-icon"
            onClick={() => setErrorMsg(null)}
            aria-label="关闭提示"
          >
            <UiIcon name="close" size={14} />
          </button>
        </div>
      )}

      {/* 便签标签管理栏 */}
      <div className="editor-tags-bar" aria-label="便签标签管理">
        <div className="editor-tags-list">
          <span className="editor-tags-label">标签:</span>
          {note.tags && note.tags.length > 0 ? (
            note.tags.map((tag) => (
              <span
                key={tag.id}
                className="tag-badge editor-tag-item"
                style={{ borderColor: tag.color || 'var(--border-color)' }}
              >
                #{tag.name}
                <button
                  type="button"
                  className="tag-remove-btn"
                  onClick={() => handleRemoveTag(tag.id)}
                  aria-label={`移除标签 ${tag.name}`}
                  title={`移除标签 ${tag.name}`}
                >
                  <UiIcon name="close" size={10} />
                </button>
              </span>
            ))
          ) : (
            <span className="no-tags-hint">暂无标签</span>
          )}
        </div>

        <div className="editor-tag-actions">
          <select
            className="tag-select-input"
            value=""
            onChange={(e) => {
              if (e.target.value) {
                handleAddTag(e.target.value)
              }
            }}
            aria-label="添加已有标签"
          >
            <option value="" disabled>
              + 添加标签...
            </option>
            {tags
              .filter((t) => !note.tags?.some((ct) => ct.id === t.id))
              .map((t) => (
                <option key={t.id} value={t.id}>
                  #{t.name}
                </option>
              ))}
          </select>
          <button
            type="button"
            className="btn"
            style={{ padding: '2px 8px', fontSize: '11px', marginLeft: '6px' }}
            onClick={onOpenTagModal}
          >
            管理标签
          </button>
        </div>
      </div>

      {/* 固定工具栏 */}
      {editor && (
        <div className="editor-toolbar" role="toolbar" aria-label="富文本编辑器工具栏">
          <div className="toolbar-group">
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('heading', { level: 1 }) ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}
              aria-label="一级标题"
              title="一级标题"
            >
              H1
            </button>
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('heading', { level: 2 }) ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
              aria-label="二级标题"
              title="二级标题"
            >
              H2
            </button>
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('heading', { level: 3 }) ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
              aria-label="三级标题"
              title="三级标题"
            >
              H3
            </button>
          </div>

          <div className="toolbar-group">
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('bold') ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleBold().run()}
              title="加粗 (Ctrl+B)"
            >
              <b>B</b>
            </button>
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('italic') ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleItalic().run()}
              title="斜体 (Ctrl+I)"
            >
              <i>I</i>
            </button>
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('link') ? 'is-active' : ''}`}
              onClick={handleSetLink}
              aria-label="添加链接"
              title="添加链接"
            >
              <UiIcon name="link" size={14} />
            </button>
          </div>

          <div className="toolbar-group">
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('bulletList') ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleBulletList().run()}
              aria-label="无序列表"
              title="无序列表"
            >
              <UiIcon name="list" size={15} />
            </button>
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('orderedList') ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleOrderedList().run()}
              aria-label="有序列表"
              title="有序列表"
            >
              <UiIcon name="list-ordered" size={15} />
            </button>
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('taskList') ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleTaskList().run()}
              aria-label="任务列表"
              title="任务列表"
            >
              <UiIcon name="check-square" size={14} />
            </button>
          </div>

          <div className="toolbar-group">
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('blockquote') ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleBlockquote().run()}
              aria-label="引用"
              title="引用"
            >
              <UiIcon name="quote" size={14} />
            </button>
            <button
              type="button"
              className={`toolbar-btn ${editor.isActive('code') ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleCode().run()}
              aria-label="行内代码"
              title="行内代码"
            >
              <UiIcon name="code" size={14} />
            </button>
            <button
              type="button"
              className={`toolbar-btn toolbar-btn-text ${editor.isActive('codeBlock') ? 'is-active' : ''}`}
              onClick={() => editor.chain().focus().toggleCodeBlock().run()}
              aria-label="代码块"
              title="代码块"
            >
              <UiIcon name="code-block" size={14} />
              <span>代码块</span>
            </button>
          </div>

          <div className="toolbar-group">
            <button
              type="button"
              className="toolbar-btn toolbar-btn-text"
              onClick={handlePasteImage}
              aria-label="插入剪贴板图片"
              title="插入剪贴板图片"
            >
              <UiIcon name="image" size={14} />
              <span>贴图</span>
            </button>
          </div>

          <div className="toolbar-group">
            <button
              type="button"
              className="toolbar-btn"
              disabled={!editor.can().undo()}
              onClick={() => editor.chain().focus().undo().run()}
              aria-label="撤销 (Ctrl+Z)"
              title="撤销 (Ctrl+Z)"
            >
              <UiIcon name="undo" size={14} />
            </button>
            <button
              type="button"
              className="toolbar-btn"
              disabled={!editor.can().redo()}
              onClick={() => editor.chain().focus().redo().run()}
              aria-label="重做 (Ctrl+Y)"
              title="重做 (Ctrl+Y)"
            >
              <UiIcon name="redo" size={14} />
            </button>
          </div>
        </div>
      )}

      {/* Tiptap 正文容器 */}
      <div className="editor-content-wrapper">
        <EditorContent editor={editor} className="tiptap-container" />
      </div>
    </div>
  )
}
