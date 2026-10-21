import { cn } from '#/lib/utils'

export type SourcesTab = 'catalog' | 'activity'

const TABS: readonly { tab: SourcesTab; label: string }[] = [
	{ tab: 'catalog', label: 'Catalogue' },
	{ tab: 'activity', label: 'Activité' },
]

interface SourcesTabsProps {
	active: SourcesTab
	onSelect: (tab: SourcesTab) => void
}

export function SourcesTabs({ active, onSelect }: SourcesTabsProps) {
	return (
		<nav aria-label="Sections des sources" className="flex gap-1 border-b">
			{TABS.map(({ tab, label }) => (
				<button
					key={tab}
					type="button"
					aria-current={tab === active ? 'page' : undefined}
					onClick={() => onSelect(tab)}
					className={cn(
						'-mb-px border-b-2 px-3 py-2 text-sm font-medium',
						tab === active
							? 'border-primary text-foreground'
							: 'border-transparent text-muted-foreground hover:text-foreground',
					)}
				>
					{label}
				</button>
			))}
		</nav>
	)
}
