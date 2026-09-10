export interface Attachment {
  id: string
  note_id: string
  relative_path: string
  mime_type: string
  byte_size: number
  width: number | null
  height: number | null
  sha256: string
  created_at: string
}
