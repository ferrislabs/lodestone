import { Badge } from '#/components/ui/badge'
import {
	authLabel,
	capabilityLabels,
} from '#/pages/source-providers/lib/labels'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { StatusBadge } from '#/pages/source-providers/ui/status-badge'

interface ProviderCardProps {
	provider: ProviderSummary
	onOpen: (key: string) => void
}

export function ProviderCard({ provider, onOpen }: ProviderCardProps) {
	const capabilities = capabilityLabels(provider)
	return (
		<button
			type="button"
			onClick={() => onOpen(provider.key)}
			className="flex h-full flex-col gap-3 rounded-xl border bg-card p-4 text-left shadow-xs transition-colors hover:bg-accent/40 focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
		>
			<span className="flex items-start justify-between gap-2">
				<span className="text-base font-semibold">{provider.name}</span>
				<StatusBadge status={provider.status} />
			</span>
			<span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
				{authLabel(provider.auth_kind)}
			</span>
			<span className="text-sm text-muted-foreground">
				{provider.description}
			</span>
			{provider.connected_account ? (
				<span className="text-xs text-muted-foreground">
					Compte : {provider.connected_account.display_name}
				</span>
			) : null}
			{capabilities.length > 0 ? (
				<span className="mt-auto flex flex-wrap gap-1">
					{capabilities.map((label) => (
						<Badge key={label} variant="secondary">
							{label}
						</Badge>
					))}
				</span>
			) : null}
		</button>
	)
}
