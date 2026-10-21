import { ShieldCheck } from 'lucide-react'

import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Skeleton } from '#/components/ui/skeleton'
import { cn } from '#/lib/utils'
import {
	categoryCounts,
	filterProviders,
} from '#/pages/source-providers/lib/filter'
import { statusLabel } from '#/pages/source-providers/lib/labels'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { ProviderCard } from '#/pages/source-providers/ui/provider-card'
import {
	type SourcesTab,
	SourcesTabs,
} from '#/pages/source-providers/ui/sources-tabs'

const STATUS_LEGEND = [
	{ status: 'ready', swatch: 'border-emerald-500' },
	{ status: 'untested', swatch: 'border-amber-500' },
	{ status: 'failing', swatch: 'border-red-500' },
	{ status: 'not_configured', swatch: 'border-dashed border-border' },
] as const

const SKELETON_CARDS = ['a', 'b', 'c']

interface CatalogPageProps {
	providers: readonly ProviderSummary[]
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	category: string | null
	query: string
	status: string | null
	onStatusChange: (status: string | null) => void
	onCategoryChange: (category: string | null) => void
	onQueryChange: (query: string) => void
	onReset: () => void
	onOpen: (key: string) => void
	onSelectTab: (tab: SourcesTab) => void
	onOpenRules: () => void
}

export function CatalogPage({
	providers,
	state,
	errorMessage,
	category,
	query,
	status,
	onStatusChange,
	onCategoryChange,
	onQueryChange,
	onReset,
	onOpen,
	onSelectTab,
	onOpenRules,
}: CatalogPageProps) {
	const visible = filterProviders(providers, { category, status, query })
	const counts = categoryCounts(providers, query)
	const totalCount = counts.reduce((sum, entry) => sum + entry.count, 0)
	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
			<div className="flex flex-wrap items-start justify-between gap-4">
				<div className="flex flex-col gap-1">
					<h1 className="text-2xl font-bold tracking-tight">
						Catalogue des sources
					</h1>
					<p className="text-sm text-muted-foreground">
						Chaque source se configure une seule fois pour toute l'organisation.
						Elle n'est utilisable par les agents qu'après un test de connexion
						réussi.
					</p>
				</div>
				<Button type="button" variant="outline" onClick={onOpenRules}>
					<ShieldCheck aria-hidden="true" />
					Règles de collecte
				</Button>
			</div>
			<SourcesTabs active="catalog" onSelect={onSelectTab} />
			{state === 'error' ? (
				<p role="alert" className="text-sm text-destructive">
					{errorMessage ?? 'Impossible de charger le catalogue'}
				</p>
			) : null}
			{state === 'loading' ? (
				<div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-4">
					{SKELETON_CARDS.map((card) => (
						<Skeleton key={card} className="aspect-square w-full" />
					))}
				</div>
			) : null}
			{state === 'ready' ? (
				<>
					<div className="flex flex-col gap-3">
						<Input
							type="search"
							value={query}
							onChange={(event) => onQueryChange(event.target.value)}
							placeholder="Rechercher une source…"
							aria-label="Rechercher une source"
							className="max-w-md"
						/>
						<fieldset className="m-0 flex min-w-0 flex-wrap gap-2 border-0 p-0">
							<legend className="sr-only">Filtrer par type</legend>
							<FilterChip
								label="Tous"
								count={totalCount}
								pressed={category === null && status === null}
								onClick={() => {
									onCategoryChange(null)
									onStatusChange(null)
								}}
							/>
							{counts.map((entry) => (
								<FilterChip
									key={entry.category}
									label={entry.title}
									count={entry.count}
									pressed={category === entry.category}
									onClick={() =>
										onCategoryChange(
											category === entry.category ? null : entry.category,
										)
									}
								/>
							))}
						</fieldset>
						<fieldset className="m-0 flex min-w-0 flex-wrap gap-2 border-0 p-0">
							<legend className="sr-only">Filtrer par statut</legend>
							{STATUS_LEGEND.map((entry) => (
								<button
									key={entry.status}
									type="button"
									aria-pressed={status === entry.status}
									onClick={() =>
										onStatusChange(
											status === entry.status ? null : entry.status,
										)
									}
									className={cn(
										'flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50',
										status === entry.status && 'bg-accent text-foreground',
									)}
								>
									<span
										aria-hidden="true"
										className={cn('size-3 rounded-sm border-2', entry.swatch)}
									/>
									{statusLabel(entry.status)}
								</button>
							))}
						</fieldset>
					</div>
					{visible.length === 0 ? (
						<div className="flex flex-col items-start gap-3">
							<p className="text-sm text-muted-foreground">
								Aucune source ne correspond.
							</p>
							<Button variant="outline" size="sm" onClick={onReset}>
								Réinitialiser les filtres
							</Button>
						</div>
					) : (
						<div
							data-testid="provider-grid"
							className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-4"
						>
							{visible.map((provider) => (
								<ProviderCard
									key={provider.key}
									provider={provider}
									onOpen={onOpen}
									onFilterCategory={onCategoryChange}
								/>
							))}
						</div>
					)}
				</>
			) : null}
		</div>
	)
}

interface FilterChipProps {
	label: string
	count: number
	pressed: boolean
	onClick: () => void
}

function FilterChip({ label, count, pressed, onClick }: FilterChipProps) {
	return (
		<Button
			type="button"
			size="sm"
			variant={pressed ? 'default' : 'outline'}
			aria-pressed={pressed}
			aria-label={`${label} ${count}`}
			onClick={onClick}
			className="rounded-full"
		>
			{label}
			<span className="tabular-nums opacity-70">{count}</span>
		</Button>
	)
}
