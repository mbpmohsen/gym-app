import { Toaster as Sonner, type ToasterProps } from 'sonner'

import { useTheme } from '@/lib/theme'

function Toaster(props: ToasterProps) {
  const { mode } = useTheme()
  return (
    <Sonner
      theme={mode}
      dir="rtl"
      position="bottom-left"
      className="toaster group"
      style={
        {
          '--normal-bg': 'var(--popover)',
          '--normal-text': 'var(--popover-foreground)',
          '--normal-border': 'var(--border)',
          fontFamily: 'var(--font-sans)',
        } as React.CSSProperties
      }
      {...props}
    />
  )
}

export { Toaster }
