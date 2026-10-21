import { Badge } from '#/components/ui/badge'
import { cn } from '#/lib/utils'
import {
	authLabel,
	capabilityLabels,
	categoryLabel,
	statusLabel,
} from '#/pages/source-providers/lib/labels'
import { providerIcon } from '#/pages/source-providers/lib/provider-icons'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'

const BORDERS: Readonly<Record<string, string>> = {
	ready: 'border-emerald-500/70 hover:border-emerald-500',
	untested: 'border-amber-500/70 hover:border-amber-500',
	failing: 'border-red-500/70 hover:border-red-500',
	not_configured: 'border-dashed border-border hover:border-foreground/40',
}

interface ProviderCardProps {
	provider: ProviderSummary
	onOpen: (key: string) => void
	onFilterCategory: (category: string) => void
}

export function ProviderCard({
	provider,
	onOpen,
	onFilterCategory,
}: ProviderCardProps) {
	const capabilities = capabilityLabels(provider)
	const icon = providerIcon(provider.key)
	const category = categoryLabel(provider.category)
	const status = statusLabel(provider.status)
	const label = `${provider.name} — ${status}`
	return (
		<div
			data-testid="provider-card"
			data-status={provider.status}
			title={label}
			className={cn(
				'relative flex aspect-square flex-col items-center gap-1.5 overflow-hidden rounded-xl border-2 bg-card p-3 text-center shadow-xs transition-colors hover:bg-accent/40 has-[a:focus-visible]:ring-[3px] has-[a:focus-visible]:ring-ring/50',
				BORDERS[provider.status] ?? BORDERS.not_configured,
			)}
		>
			<span className="flex size-14 shrink-0 items-center justify-center rounded-lg bg-white p-2 ring-1 ring-border">
				{icon.kind === 'logo' ? (
					<img
						src={icon.src}
						alt={provider.name}
						className="size-full object-contain"
					/>
				) : (
					<icon.Icon aria-hidden="true" className="size-8 text-neutral-700" />
				)}
			</span>
			<a
				aria-label={label}
				href={`/sources/providers/${provider.key}`}
				onClick={(event) => {
					if (
						event.defaultPrevented ||
						event.button !== 0 ||
						event.metaKey ||
						event.ctrlKey ||
						event.shiftKey ||
						event.altKey
					)
						return
					event.preventDefault()
					onOpen(provider.key)
				}}
				className="text-sm leading-tight font-semibold shrink-0 outline-none after:absolute after:inset-0 after:content-['']"
			>
				{provider.name}
			</a>
			<button
				type="button"
				aria-label={`Filtrer par ${category}`}
				onClick={(event) => {
					event.stopPropagation()
					onFilterCategory(provider.category)
				}}
				className="relative z-10 rounded-full outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
			>
				<Badge variant="secondary" className="cursor-pointer">
					{category}
				</Badge>
			</button>
			<span className="text-[0.65rem] font-medium tracking-wide text-muted-foreground uppercase">
				{authLabel(provider.auth_kind)}
			</span>
			<span
				title={provider.description}
				className="line-clamp-2 shrink-0 text-xs text-muted-foreground"
			>
				{provider.description}
			</span>
			{provider.connected_account ? (
				<span className="text-xs text-muted-foreground">
					Compte : {provider.connected_account.display_name}
				</span>
			) : null}
			{capabilities.length > 0 ? (
				<span className="mt-auto flex shrink-0 flex-wrap justify-center gap-1">
					{capabilities.map((label) => (
						<Badge key={label} variant="outline">
							{label}
						</Badge>
					))}
				</span>
			) : null}
		</div>
	)
}
