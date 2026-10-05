import type { TabItem } from '#/pages/profiles/lib/tabs'

interface TabBarProps<T extends string> {
	tabs: readonly TabItem<T>[]
	active: T
	onChange: (tab: T) => void
}

export function TabBar<T extends string>({
	tabs,
	active,
	onChange,
}: TabBarProps<T>) {
	return (
		<div className="flex flex-wrap gap-1 border-b" role="tablist">
			{tabs.map((item) => (
				<button
					key={item.id}
					type="button"
					role="tab"
					aria-selected={active === item.id}
					className="flex items-center gap-1.5 border-b-2 border-transparent px-3 py-2 text-sm font-medium text-muted-foreground aria-selected:border-primary aria-selected:text-foreground"
					onClick={() => onChange(item.id)}
				>
					{item.label}
					{item.problems > 0 ? (
						<span
							role="img"
							aria-label={`${item.problems} ${item.problems > 1 ? 'problèmes' : 'problème'}`}
							className="inline-flex min-w-5 items-center justify-center rounded-full bg-destructive px-1.5 text-xs text-white"
						>
							{item.problems}
						</span>
					) : null}
				</button>
			))}
		</div>
	)
}
