import { Skeleton } from '#/components/ui/skeleton'
import { groupByCategory } from '#/pages/source-providers/lib/labels'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { ProviderCard } from '#/pages/source-providers/ui/provider-card'
import {
	type SourcesTab,
	SourcesTabs,
} from '#/pages/source-providers/ui/sources-tabs'

const SKELETON_CARDS = ['a', 'b', 'c']

interface CatalogPageProps {
	providers: readonly ProviderSummary[]
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	onOpen: (key: string) => void
	onSelectTab: (tab: SourcesTab) => void
}

export function CatalogPage({
	providers,
	state,
	errorMessage,
	onOpen,
	onSelectTab,
}: CatalogPageProps) {
	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
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
			<SourcesTabs active="catalog" onSelect={onSelectTab} />
			{state === 'error' ? (
				<p role="alert" className="text-sm text-destructive">
					{errorMessage ?? 'Impossible de charger le catalogue'}
				</p>
			) : null}
			{state === 'loading' ? (
				<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
					{SKELETON_CARDS.map((card) => (
						<Skeleton key={card} className="h-36 w-full" />
					))}
				</div>
			) : null}
			{state === 'ready'
				? groupByCategory(providers).map((group) => (
						<section
							key={group.category}
							aria-label={group.title}
							className="flex flex-col gap-3"
						>
							<h2 className="text-base font-semibold">{group.title}</h2>
							<div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
								{group.providers.map((provider) => (
									<ProviderCard
										key={provider.key}
										provider={provider}
										onOpen={onOpen}
									/>
								))}
							</div>
						</section>
					))
				: null}
		</div>
	)
}
