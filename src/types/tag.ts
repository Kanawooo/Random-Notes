export interface Tag {
  id: string
  name: string
  normalized_name: string
  color: string
  created_at: string
}

export interface CreateTagInput {
  name: string
  color?: string
}

export interface RenameTagInput {
  id: string
  name: string
}

export interface AssignTagsInput {
  noteId: string
  tagIds: string[]
}
