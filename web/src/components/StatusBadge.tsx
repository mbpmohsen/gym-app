import { Badge } from '@/components/ui/badge'
import type { Status } from '@/lib/types'

// expired / debt / none all play "پایان شهریه" at the door (SPEC §4.2)
const MAP: Record<Status, { label: string; variant: 'success' | 'warning' | 'danger' | 'secondary' }> = {
  ok: { label: 'شهریه دارد', variant: 'success' },
  debt: { label: 'بدهکار', variant: 'danger' },
  expired: { label: 'پایان شهریه', variant: 'danger' },
  none: { label: 'بدون اشتراک', variant: 'secondary' },
}

export function StatusBadge({ status }: { status: Status }) {
  const s = MAP[status]
  return <Badge variant={s.variant}>{s.label}</Badge>
}
