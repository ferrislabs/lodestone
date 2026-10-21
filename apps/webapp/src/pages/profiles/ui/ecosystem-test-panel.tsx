import { ModuleFindings } from '#/components/ecosystem/module-findings'
import { ModuleStatusChip } from '#/components/ecosystem/module-status-chip'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { cn } from '#/lib/utils'
import type { DryRunView } from '#/pages/profiles/lib/enrichment'

export interface LeadOption {
	id: string
	domain: string
}

export interface EcosystemTestState {
	module: string
	moduleLabel: string
	query: string
	options: readonly LeadOption[]
	searching: boolean
	selectedId: string | null
	running: boolean
	result: DryRunView | null
	error: string | null
	unavailableReason: string | null
}

export interface EcosystemTestActions {
	onClose: () => void
	onQuery: (query: string) => void
	onPick: (leadId: string) => void
	onRun: () => void
}

interface EcosystemTestPanelProps {
	test: EcosystemTestState
	actions: EcosystemTestActions
}

export function EcosystemTestPanel({ test, actions }: EcosystemTestPanelProps) {
	const result = test.result
	return (
		<section
			aria-label={`Tester ${test.moduleLabel} sur un lead`}
			data-testing={test.module}
			className="flex flex-col gap-3 rounded-lg border border-dashed p-4"
		>
			<div className="flex items-center justify-between gap-2">
				<h4 className="text-sm font-semibold">Tester sur un lead</h4>
				<Button variant="ghost" size="sm" onClick={actions.onClose}>
					Fermer
				</Button>
			</div>
			{test.unavailableReason ? (
				<p className="text-sm text-muted-foreground">
					{test.unavailableReason}
				</p>
			) : (
				<>
					<p className="text-xs text-muted-foreground">
						Le module tourne tout de suite sur le domaine du lead choisi. Rien
						n'est enregistré.
					</p>
					<Input
						aria-label="Rechercher un lead par domaine"
						placeholder="Rechercher un lead par domaine"
						value={test.query}
						onChange={(event) => actions.onQuery(event.target.value)}
					/>
					{test.searching ? (
						<p className="text-xs text-muted-foreground">Recherche…</p>
					) : test.options.length === 0 ? (
						<p className="text-xs text-muted-foreground">
							Aucun lead ne correspond.
						</p>
					) : (
						<ul
							aria-label="Leads du profil"
							className="flex max-h-48 flex-col overflow-y-auto rounded-md border"
						>
							{test.options.map((option) => (
								<li key={option.id}>
									<button
										type="button"
										aria-pressed={option.id === test.selectedId}
										onClick={() => actions.onPick(option.id)}
										className={cn(
											'w-full px-3 py-1.5 text-left text-sm hover:bg-muted',
											option.id === test.selectedId && 'bg-muted font-medium',
										)}
									>
										{option.domain}
									</button>
								</li>
							))}
						</ul>
					)}
					<div>
						<Button
							size="sm"
							disabled={test.selectedId === null || test.running}
							onClick={actions.onRun}
						>
							{test.running ? 'Test en cours…' : 'Lancer le test'}
						</Button>
					</div>
				</>
			)}
			{test.error ? (
				<p role="alert" className="text-sm text-destructive">
					{test.error}
				</p>
			) : null}
			{result ? (
				<div className="flex flex-col gap-2" data-section="dry-run-result">
					<div className="flex flex-wrap items-center gap-2">
						<ModuleStatusChip label="Résultat du test" tone={result.tone} />
						<span className="text-xs text-muted-foreground">
							{result.summary}
						</span>
					</div>
					{result.notice ? (
						<p className="text-sm text-muted-foreground">{result.notice}</p>
					) : null}
					<ModuleFindings view={result.view} empty="Aucun élément trouvé." />
				</div>
			) : null}
		</section>
	)
}
