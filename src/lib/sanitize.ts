import { toAttachmentCanonicalSrc } from './attachmentSrc'

export function sanitizePastedHtml(html: string): string {
  if (!html || typeof html !== 'string') return ''

  const parser = new DOMParser()
  const doc = parser.parseFromString(html, 'text/html')

  // Remove dangerous tags
  const dangerousTags = ['script', 'iframe', 'object', 'embed', 'form', 'svg', 'link', 'meta', 'style']
  dangerousTags.forEach((tag) => {
    const elements = doc.querySelectorAll(tag)
    elements.forEach((el) => el.remove())
  })

  // Sanitize images: only allow attachment refs (canonical or WebView2 display form, normalized back to canonical)
  const images = doc.querySelectorAll('img')
  images.forEach((img) => {
    const src = img.getAttribute('src') || ''
    const canonicalSrc = toAttachmentCanonicalSrc(src)
    if (!canonicalSrc.startsWith('suijian-attachment://')) {
      img.remove()
    } else {
      img.setAttribute('src', canonicalSrc)
      // Strip potentially dangerous attributes
      for (const attr of Array.from(img.attributes)) {
        if (!['src', 'alt', 'title', 'width', 'height', 'class'].includes(attr.name.toLowerCase())) {
          img.removeAttribute(attr.name)
        }
      }
    }
  })

  // Sanitize links: only allow http, https, mailto
  const links = doc.querySelectorAll('a')
  links.forEach((a) => {
    const href = a.getAttribute('href') || ''
    const trimmed = href.trim().toLowerCase()
    if (!trimmed.startsWith('http://') && !trimmed.startsWith('https://') && !trimmed.startsWith('mailto:')) {
      a.removeAttribute('href')
    }
  })

  return doc.body.innerHTML
}
