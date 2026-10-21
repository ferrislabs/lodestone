import { Skeleton } from '#/components/ui/skeleton'
import {
	Table,
	TableBody,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
import { groupedSources } from '#/pages/sources/lib/state'
import type { Source } from '#/pages/sources/lib/types'
import { ListenerRow } from '#/pages/sources/ui/listener-row'
import { SourceRow } from '#/pages/sources/ui/source-row'

const SKELETON_ROWS = ['a', 'b', 'c', 'd']

interface SourcesPageProps {
	sources: readonly Source[]
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	now: number
	scheduled: ReadonlySet<string>
	pending: ReadonlySet<string>
	errors: Readonly<Record<string, string>>
	onRun: (source: string) => void
}

export function SourcesPage({
	sources,
	state,
	errorMessage,
	now,
	scheduled,
	pending,
	errors,
	onRun,
}: SourcesPageProps) {
	return (
		<div className="flex flex-col gap-6 p-4 md:p-8">
			<div className="flex flex-col gap-1">
				<h1 className="text-2xl font-bold tracking-tight">Sources</h1>
				<p className="text-sm text-muted-foreground">
					Les sources en écoute continue tournent en permanence. Les autres
					sources tournent sur leur propre tâche récurrente : « Lancer
					maintenant » avance le prochain passage à l'instant présent pour tous
					les profils.
				</p>
			</div>
			{state === 'error' ? (
				<p role="alert" className="text-sm text-destructive">
					{errorMessage ?? 'Impossible de charger les sources'}
				</p>
			) : null}
			{state === 'loading'
				? SKELETON_ROWS.map((row) => (
						<Skeleton key={row} className="h-16 w-full" />
					))
				: null}
			{state === 'ready'
				? groupedSources(sources).map((group) => (
						<section
							key={group.title}
							aria-label={group.title}
							className="flex flex-col gap-2"
						>
							<h2 className="text-base font-semibold">{group.title}</h2>
							<div className="rounded-xl border bg-card">
								{group.continuous ? (
									<Table>
										<TableHeader>
											<TableRow>
												<TableHead>Source</TableHead>
												<TableHead>État</TableHead>
												<TableHead>Dernière activité</TableHead>
												<TableHead>File d'attente</TableHead>
												<TableHead>Totaux</TableHead>
												<TableHead>Dernier passage</TableHead>
											</TableRow>
										</TableHeader>
										<TableBody>
											{group.sources.map((source) =>
												source.listener ? (
													<ListenerRow
														key={source.source}
														source={source}
														listener={source.listener}
														now={now}
													/>
												) : null,
											)}
										</TableBody>
									</Table>
								) : (
									<Table>
										<TableHeader>
											<TableRow>
												<TableHead>Source</TableHead>
												<TableHead>État</TableHead>
												<TableHead>Prochain passage</TableHead>
												<TableHead>Dernier passage</TableHead>
												<TableHead>Action</TableHead>
											</TableRow>
										</TableHeader>
										<TableBody>
											{group.sources.map((source) => (
												<SourceRow
													key={source.source}
													source={source}
													now={now}
													scheduled={scheduled.has(source.source)}
													pending={pending.has(source.source)}
													error={errors[source.source] ?? null}
													onRun={onRun}
												/>
											))}
										</TableBody>
									</Table>
								)}
							</div>
						</section>
					))
				: null}
		</div>
	)
}
