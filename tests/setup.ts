import '@testing-library/jest-dom/vitest'
import * as matchers from '@testing-library/jest-dom/matchers'
import { expect, vi } from 'vitest'

expect.extend(matchers)

if (typeof document !== 'undefined') {
  document.elementFromPoint = document.elementFromPoint || vi.fn(() => null)
}
