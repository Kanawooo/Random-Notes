// 附件 URL 映射：文档 JSON 始终保存 canonical 形式 suijian-attachment://<uuid>。
// Windows WebView2 只拦截 http://<scheme>.localhost/<path> 形式的请求（wry 自定义协议 workaround），
// 因此仅在渲染时转换为显示形式；反向转换用于粘贴净化时把显示形式归一化回 canonical。
const CANONICAL_PREFIX = 'suijian-attachment://'
const DISPLAY_PREFIX = 'http://suijian-attachment.localhost/'

export function toAttachmentDisplaySrc(src: string): string {
  if (!src.startsWith(CANONICAL_PREFIX)) {
    return src
  }
  const id = src.slice(CANONICAL_PREFIX.length).replace(/^localhost\//, '')
  return `${DISPLAY_PREFIX}${id}`
}

export function toAttachmentCanonicalSrc(src: string): string {
  if (src.startsWith(DISPLAY_PREFIX)) {
    return `${CANONICAL_PREFIX}${src.slice(DISPLAY_PREFIX.length)}`
  }
  return src
}
