import type { ReactNode } from 'react'

interface HelpPanelProps {
	title: string
	collapsed: boolean
	onToggle: () => void
	children: ReactNode
}

export function HelpPanel({
	title,
	collapsed,
	onToggle,
	children,
}: HelpPanelProps) {
	return (
		<aside
			aria-label={title}
			data-testid="help-panel"
			className="flex flex-col gap-3 rounded-xl border bg-card p-4 text-sm shadow-sm lg:sticky lg:top-4 lg:order-2 lg:col-span-1 lg:self-start"
		>
			<button
				type="button"
				aria-expanded={!collapsed}
				className="flex w-full items-center justify-between text-left font-semibold lg:pointer-events-none"
				onClick={onToggle}
			>
				{title}
				<span aria-hidden="true" className="text-muted-foreground lg:hidden">
					{collapsed ? '+' : '−'}
				</span>
			</button>
			<div
				className={`flex flex-col gap-3 ${collapsed ? 'hidden lg:flex' : ''}`}
			>
				{children}
			</div>
		</aside>
	)
}
