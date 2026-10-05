import type { ReactNode } from 'react'

import { Button } from '#/components/ui/button'

interface LlmOverviewProps {
	tierPanel: ReactNode
	providers: ReactNode
	onCreate: () => void
}

export function LlmOverview({
	tierPanel,
	providers,
	onCreate,
}: LlmOverviewProps) {
	return (
		<div className="flex flex-col gap-4 p-4 md:p-8">
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div className="flex flex-col gap-1">
					<h1 className="text-2xl font-bold tracking-tight">LLM providers</h1>
					<p className="text-sm text-muted-foreground">
						Providers answer the triage and qualification tiers. API keys are
						write-only and never shown.
					</p>
				</div>
				<Button onClick={onCreate}>New provider</Button>
			</div>
			{tierPanel}
			{providers}
		</div>
	)
}
