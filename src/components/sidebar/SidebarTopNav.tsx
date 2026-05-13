import { Archive, CheckSquareOffset, FileText, Tray } from '@phosphor-icons/react'
import type { SidebarSelection } from '../../types'
import { isSelectionActive, NavItem } from '../SidebarParts'
import { translate, type AppLocale } from '../../lib/i18n'

interface SidebarTopNavProps {
  selection: SidebarSelection
  onSelect: (selection: SidebarSelection) => void
  showInbox: boolean
  inboxCount: number
  activeCount: number
  archivedCount: number
  openTaskCount: number
  locale?: AppLocale
  loading?: boolean
}

export function SidebarTopNav({
  selection,
  onSelect,
  showInbox,
  inboxCount,
  activeCount,
  archivedCount,
  openTaskCount,
  locale = 'en',
  loading = false,
}: SidebarTopNavProps) {
  return (
    <div className="border-b border-border" data-testid="sidebar-top-nav" style={{ padding: '4px 6px' }}>
      {showInbox && (
        <NavItem
          icon={Tray}
          label={translate(locale, 'sidebar.nav.inbox')}
          count={inboxCount}
          countLoading={loading}
          isActive={isSelectionActive(selection, { kind: 'filter', filter: 'inbox' })}
          badgeClassName="text-muted-foreground"
          badgeStyle={{ background: 'var(--muted)' }}
          activeBadgeClassName="bg-primary text-primary-foreground"
          onClick={() => onSelect({ kind: 'filter', filter: 'inbox' })}
        />
      )}
      <NavItem
        icon={CheckSquareOffset}
        label={translate(locale, 'sidebar.nav.tasks')}
        count={openTaskCount}
        countLoading={loading}
        isActive={isSelectionActive(selection, { kind: 'filter', filter: 'tasks' })}
        badgeClassName="text-muted-foreground"
        badgeStyle={{ background: 'var(--muted)' }}
        activeBadgeClassName="bg-primary text-primary-foreground"
        onClick={() => onSelect({ kind: 'filter', filter: 'tasks' })}
      />
      <NavItem
        icon={FileText}
        label={translate(locale, 'sidebar.nav.allNotes')}
        count={activeCount}
        countLoading={loading}
        isActive={isSelectionActive(selection, { kind: 'filter', filter: 'all' })}
        badgeClassName="text-muted-foreground"
        badgeStyle={{ background: 'var(--muted)' }}
        activeBadgeClassName="bg-primary text-primary-foreground"
        onClick={() => onSelect({ kind: 'filter', filter: 'all' })}
      />
      <NavItem
        icon={Archive}
        label={translate(locale, 'sidebar.nav.archive')}
        count={archivedCount}
        countLoading={loading}
        isActive={isSelectionActive(selection, { kind: 'filter', filter: 'archived' })}
        badgeClassName="text-muted-foreground"
        badgeStyle={{ background: 'var(--muted)' }}
        activeBadgeClassName="bg-primary text-primary-foreground"
        onClick={() => onSelect({ kind: 'filter', filter: 'archived' })}
      />
    </div>
  )
}
