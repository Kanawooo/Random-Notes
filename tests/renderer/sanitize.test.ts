import { describe, it, expect } from 'vitest'
import { sanitizePastedHtml } from '../../src/lib/sanitize'

describe('sanitizePastedHtml', () => {
  it('strips dangerous tags like script, iframe, svg, object, embed, style', () => {
    const malicious = `
      <div>
        <p>Hello world</p>
        <script>alert(1)</script>
        <iframe src="http://example.com"></iframe>
        <svg><text>test</text></svg>
        <style>body { display: none; }</style>
      </div>
    `
    const sanitized = sanitizePastedHtml(malicious)
    expect(sanitized).not.toContain('<script>')
    expect(sanitized).not.toContain('<iframe>')
    expect(sanitized).not.toContain('<svg>')
    expect(sanitized).not.toContain('<style>')
    expect(sanitized).toContain('<p>Hello world</p>')
  })

  it('removes images with non-suijian-attachment scheme', () => {
    const html = `
      <div>
        <img src="http://example.com/evil.png" />
        <img src="file:///C:/passwords.txt" />
        <img src="data:image/png;base64,123" />
        <img src="suijian-attachment://12345678-1234-4234-8234-1234567890ab" alt="safe" />
      </div>
    `
    const sanitized = sanitizePastedHtml(html)
    expect(sanitized).not.toContain('http://example.com/evil.png')
    expect(sanitized).not.toContain('file:///C:/passwords.txt')
    expect(sanitized).not.toContain('data:image/png')
    expect(sanitized).toContain('src="suijian-attachment://12345678-1234-4234-8234-1234567890ab"')
  })

  it('strips dangerous links like javascript: or data:', () => {
    const html = `
      <div>
        <a href="javascript:alert(1)">Click me</a>
        <a href="https://example.com">Legit</a>
      </div>
    `
    const sanitized = sanitizePastedHtml(html)
    expect(sanitized).not.toContain('href="javascript:')
    expect(sanitized).toContain('href="https://example.com"')
  })

  it('handles empty or non-string inputs safely', () => {
    expect(sanitizePastedHtml('')).toBe('')
    // @ts-expect-error testing invalid type
    expect(sanitizePastedHtml(null)).toBe('')
  })
})
