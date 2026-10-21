import {
	providerName,
	providerStatusOf,
} from '#/pages/profiles/lib/search-agent'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { StatusBadge } from '#/pages/source-providers/ui/status-badge'

interface ProviderChipsProps {
	keys: readonly string[]
	catalog: readonly ProviderSummary[]
}

export function ProviderChips({ keys, catalog }: ProviderChipsProps) {
	if (keys.length === 0) {
		return (
			<p className="text-xs text-muted-foreground">Aucun fournisseur choisi.</p>
		)
	}
	return (
		<ul aria-label="Fournisseurs" className="flex flex-wrap gap-1.5">
			{keys.map((key) => (
				<li
					key={key}
					data-provider={key}
					className="flex items-center gap-1.5 rounded-full border bg-background px-2 py-0.5 text-xs"
				>
					<span>{providerName(catalog, key)}</span>
					<StatusBadge status={providerStatusOf(catalog, key)} />
				</li>
			))}
		</ul>
	)
}
