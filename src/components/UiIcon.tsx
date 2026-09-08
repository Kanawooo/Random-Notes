import React from 'react'

export type UiIconName =
  | 'search'
  | 'settings'
  | 'plus'
  | 'close'
  | 'arrow-left'
  | 'pin'
  | 'tag'
  | 'archive'
  | 'trash'
  | 'check'
  | 'check-square'
  | 'square'
  | 'image'
  | 'link'
  | 'undo'
  | 'redo'
  | 'warning'
  | 'shield'
  | 'inbox'
  | 'list'
  | 'list-ordered'
  | 'quote'
  | 'code'
  | 'code-block'
  | 'hourglass'

interface UiIconProps extends React.SVGProps<SVGSVGElement> {
  name: UiIconName
  size?: number
}

export const UiIcon: React.FC<UiIconProps> = ({
  name,
  size = 18,
  className = '',
  style,
  ...props
}) => {
  const iconStyle: React.CSSProperties = {
    display: 'inline-block',
    verticalAlign: 'middle',
    flexShrink: 0,
    ...style
  }

  const renderPath = () => {
    switch (name) {
      case 'search':
        return (
          <>
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.35-4.35" />
          </>
        )
      case 'settings':
        return (
          <>
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
          </>
        )
      case 'plus':
        return <path d="M12 5v14M5 12h14" />
      case 'close':
        return <path d="M18 6 6 18M6 6l12 12" />
      case 'arrow-left':
        return <path d="m12 19-7-7 7-7M19 12H5" />
      case 'pin':
        return (
          <>
            <path d="M12 17v5" />
            <path d="M9 2h6l-1 5 3 3v2H7v-2l3-3-1-5z" />
          </>
        )
      case 'tag':
        return (
          <>
            <path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8 8a2 2 0 0 0 2.828 0l7.172-7.172a2 2 0 0 0 0-2.828l-8-8z" />
            <circle cx="7.5" cy="7.5" r="1.5" fill="currentColor" />
          </>
        )
      case 'archive':
        return (
          <>
            <path d="M21 8v13H3V8" />
            <path d="M1 3h22v5H1z" />
            <path d="M10 12h4" />
          </>
        )
      case 'trash':
        return (
          <>
            <path d="M3 6h18" />
            <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
            <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
            <line x1="10" y1="11" x2="10" y2="17" />
            <line x1="14" y1="11" x2="14" y2="17" />
          </>
        )
      case 'check':
        return <path d="M20 6 9 17l-5-5" />
      case 'check-square':
        return (
          <>
            <rect width="18" height="18" x="3" y="3" rx="2" />
            <path d="m9 12 2 2 4-4" />
          </>
        )
      case 'square':
        return <rect width="18" height="18" x="3" y="3" rx="2" />
      case 'image':
        return (
          <>
            <rect width="18" height="18" x="3" y="3" rx="2" />
            <circle cx="8.5" cy="8.5" r="1.5" />
            <path d="m21 15-5-5L5 21" />
          </>
        )
      case 'link':
        return (
          <>
            <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
            <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
          </>
        )
      case 'undo':
        return (
          <>
            <path d="M3 7v6h6" />
            <path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13" />
          </>
        )
      case 'redo':
        return (
          <>
            <path d="M21 7v6h-6" />
            <path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3L21 13" />
          </>
        )
      case 'warning':
        return (
          <>
            <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z" />
            <line x1="12" y1="9" x2="12" y2="13" />
            <line x1="12" y1="17" x2="12.01" y2="17" />
          </>
        )
      case 'shield':
        return <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
      case 'inbox':
        return (
          <>
            <polyline points="22 12 16 12 14 15 10 15 8 12 2 12" />
            <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" />
          </>
        )
      case 'list':
        return (
          <>
            <line x1="8" y1="6" x2="21" y2="6" />
            <line x1="8" y1="12" x2="21" y2="12" />
            <line x1="8" y1="18" x2="21" y2="18" />
            <line x1="3" y1="6" x2="3.01" y2="6" />
            <line x1="3" y1="12" x2="3.01" y2="12" />
            <line x1="3" y1="18" x2="3.01" y2="18" />
          </>
        )
      case 'list-ordered':
        return (
          <>
            <line x1="10" y1="6" x2="21" y2="6" />
            <line x1="10" y1="12" x2="21" y2="12" />
            <line x1="10" y1="18" x2="21" y2="18" />
            <path d="M4 6h1v4" />
            <path d="M4 10h2" />
            <path d="M6 18H4c0-1 2-2 2-3s-1-1.5-2-1" />
          </>
        )
      case 'quote':
        return (
          <path d="M3 21c3 0 7-1 7-8V5c0-1.25-.756-2.017-2-2H4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V20c0 1 0 1 1 1zM15 21c3 0 7-1 7-8V5c0-1.25-.757-2.017-2-2h-4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V20c0 1 0 1 1 1z" />
        )
      case 'code':
        return (
          <>
            <polyline points="16 18 22 12 16 6" />
            <polyline points="8 6 2 12 8 18" />
          </>
        )
      case 'code-block':
        return (
          <>
            <rect width="18" height="18" x="3" y="3" rx="2" />
            <path d="m10 10-2 2 2 2" />
            <path d="m14 10 2 2-2 2" />
          </>
        )
      case 'hourglass':
        return (
          <>
            <path d="M5 22h14" />
            <path d="M5 2h14" />
            <path d="M17 22v-4.172a2 2 0 0 0-.586-1.414L12 12l-4.414 4.414A2 2 0 0 0 7 17.828V22" />
            <path d="M7 2v4.172a2 2 0 0 0 .586 1.414L12 12l4.414-4.414A2 2 0 0 0 17 6.172V2" />
          </>
        )
      default:
        return null
    }
  }

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`ui-icon ui-icon-${name} ${className}`}
      style={iconStyle}
      aria-hidden="true"
      {...props}
    >
      {renderPath()}
    </svg>
  )
}
