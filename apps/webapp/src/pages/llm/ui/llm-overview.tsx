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
					<h1 className="text-2xl font-bold tracking-tight">
						Fournisseurs LLM
					</h1>
					<p className="text-sm text-muted-foreground">
						Les fournisseurs par défaut servent aux agents de qualification
						(Classification) et à l'agrégateur (Synthèse) qui n'en choisissent
						pas. Les clés API sont en écriture seule et ne sont jamais
						affichées.
					</p>
				</div>
				<Button onClick={onCreate}>Nouveau fournisseur</Button>
			</div>
			{tierPanel}
			{providers}
		</div>
	)
}
