import type { Tag } from './tag'

export interface Note {
  id: string
  title: string
  content_json: string
  plain_text: string
  title_manually_edited: boolean
  is_pinned: boolean
  archived_at: string | null
  deleted_at: string | null
  revision: number
  created_at: string
  updated_at: string
  tags?: Tag[]
}

export type NoteScope = 'active' | 'archived' | 'trash' | 'all'

export interface CreateNoteInput {
  title?: string
  content_json?: string
  plain_text?: string
  title_manually_edited?: boolean
  is_pinned?: boolean
  tag_ids?: string[]
  reuse_empty_draft?: boolean
}

export interface UpdateNoteInput {
  id: string
  expectedRevision: number
  title?: string
  content_json?: string
  plain_text?: string
  title_manually_edited?: boolean
  is_pinned?: boolean
  tag_ids?: string[]
}

export interface ListNotesOptions {
  scope?: NoteScope
  limit?: number
}

export interface SearchNotesOptions {
  query: string
  scope?: NoteScope
  limit?: number
}

export interface BatchResult {
  affectedCount: number
}
