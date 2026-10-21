import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Skeleton } from '#/components/ui/skeleton'
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
import { translateApiMessage } from '#/lib/api-errors'
import {
	inFlightLabel,
	providerKindLabel,
	reasoningLabel,
	testErrorHint,
	testErrorLabel,
	tierShortLabel,
} from '#/pages/llm/lib/labels'
import { keyLabel } from '#/pages/llm/lib/tiers'
import type { Provider, TestResult } from '#/pages/llm/lib/types'

const SKELETON_ROWS = ['a', 'b', 'c']
const COLUMNS = 8

export interface TestOutcome {
	providerId: string
	providerName: string
	result: TestResult | null
	error: string | null
}

interface ProvidersTableProps {
	providers: readonly Provider[]
	state: 'loading' | 'error' | 'ready'
	errorMessage: string | null
	actionError: string | null
	busyId: string | null
	testingId: string | null
	confirmingDeleteId: string | null
	testOutcome: TestOutcome | null
	onEdit: (providerId: string) => void
	onToggleEnabled: (provider: Provider) => void
	onTest: (providerId: string) => void
	onRequestDelete: (providerId: string) => void
	onConfirmDelete: (providerId: string) => void
	onCancelDelete: () => void
}

export function ProvidersTable({
	providers,
	state,
	errorMessage,
	actionError,
	busyId,
	testingId,
	confirmingDeleteId,
	testOutcome,
	onEdit,
	onToggleEnabled,
	onTest,
	onRequestDelete,
	onConfirmDelete,
	onCancelDelete,
}: ProvidersTableProps) {
	return (
		<div className="flex flex-col gap-3">
			{actionError ? (
				<p role="alert" className="text-sm text-destructive">
					{actionError}
				</p>
			) : null}
			{testOutcome ? <TestOutcomeView outcome={testOutcome} /> : null}
			<div className="rounded-xl border bg-card">
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>Nom</TableHead>
							<TableHead>Type</TableHead>
							<TableHead>Modèle</TableHead>
							<TableHead>URL de base</TableHead>
							<TableHead>Statut</TableHead>
							<TableHead>Clé API</TableHead>
							<TableHead>Par défaut pour</TableHead>
							<TableHead className="text-right">Actions</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{state === 'loading'
							? SKELETON_ROWS.map((id) => (
									<TableRow key={id}>
										<TableCell colSpan={COLUMNS}>
											<Skeleton className="h-5 w-full" />
										</TableCell>
									</TableRow>
								))
							: null}
						{state === 'error' ? (
							<TableRow>
								<TableCell
									colSpan={COLUMNS}
									className="py-10 text-center text-destructive"
									role="alert"
								>
									Impossible de charger les fournisseurs : {errorMessage}
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready' && providers.length === 0 ? (
							<TableRow>
								<TableCell
									colSpan={COLUMNS}
									className="py-10 text-center text-muted-foreground"
								>
									Aucun fournisseur pour le moment.
								</TableCell>
							</TableRow>
						) : null}
						{state === 'ready'
							? providers.map((provider) => (
									<TableRow key={provider.id}>
										<TableCell>
											<button
												type="button"
												className="font-medium text-foreground hover:underline"
												onClick={() => onEdit(provider.id)}
											>
												{provider.name}
											</button>
										</TableCell>
										<TableCell>{providerKindLabel(provider.kind)}</TableCell>
										<TableCell>
											{provider.model}
											{provider.reasoning_effort ? (
												<span className="block text-xs text-muted-foreground">
													Raisonnement :{' '}
													{reasoningLabel(provider.reasoning_effort)}
												</span>
											) : null}
										</TableCell>
										<TableCell className="max-w-48 truncate text-muted-foreground">
											{provider.base_url}
										</TableCell>
										<TableCell>
											<Badge
												variant={provider.enabled ? 'default' : 'secondary'}
											>
												{provider.enabled ? 'Activé' : 'Désactivé'}
											</Badge>
											<span className="block text-xs text-muted-foreground">
												{inFlightLabel(
													provider.in_flight,
													provider.max_concurrency,
												)}
											</span>
										</TableCell>
										<TableCell>
											<Badge
												variant={provider.has_api_key ? 'outline' : 'secondary'}
											>
												{keyLabel(provider.has_api_key)}
											</Badge>
										</TableCell>
										<TableCell>
											<div className="flex flex-wrap gap-1">
												{provider.tiers.map((tier) => (
													<Badge key={tier} variant="secondary">
														{tierShortLabel(tier)}
													</Badge>
												))}
											</div>
										</TableCell>
										<TableCell>
											<div className="flex flex-wrap items-center justify-end gap-2">
												{confirmingDeleteId === provider.id ? (
													<>
														<Button
															size="sm"
															variant="destructive"
															disabled={busyId === provider.id}
															onClick={() => onConfirmDelete(provider.id)}
														>
															Confirmer la suppression
														</Button>
														<Button
															size="sm"
															variant="outline"
															onClick={onCancelDelete}
														>
															Annuler
														</Button>
													</>
												) : (
													<>
														<Button
															size="sm"
															variant="outline"
															disabled={testingId === provider.id}
															onClick={() => onTest(provider.id)}
														>
															{testingId === provider.id
																? 'Test en cours…'
																: 'Tester la connexion'}
														</Button>
														<Button
															size="sm"
															variant="outline"
															disabled={busyId === provider.id}
															onClick={() => onToggleEnabled(provider)}
														>
															{provider.enabled ? 'Désactiver' : 'Activer'}
														</Button>
														<Button
															size="sm"
															variant="outline"
															onClick={() => onRequestDelete(provider.id)}
														>
															Supprimer
														</Button>
													</>
												)}
											</div>
										</TableCell>
									</TableRow>
								))
							: null}
					</TableBody>
				</Table>
			</div>
		</div>
	)
}

function TestOutcomeView({ outcome }: { outcome: TestOutcome }) {
	if (outcome.error) {
		return (
			<p role="alert" className="text-sm text-destructive">
				Échec du test de {outcome.providerName} : {outcome.error}
			</p>
		)
	}
	const result = outcome.result
	if (result?.ok) {
		return (
			<output className="block text-sm">
				{outcome.providerName} : connexion réussie, modèle {result.model},
				latence {result.latency_ms} ms
			</output>
		)
	}
	const kind = result?.error?.kind ?? 'other'
	const hint = testErrorHint(kind)
	return (
		<p role="alert" className="text-sm text-destructive">
			{outcome.providerName} : {testErrorLabel(kind)} -{' '}
			{translateApiMessage(result?.error?.message ?? 'The test failed')}
			{hint ? ` ${hint}` : null}
		</p>
	)
}
