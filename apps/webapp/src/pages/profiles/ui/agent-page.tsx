import { ArrowLeftIcon, Trash2Icon } from 'lucide-react'

import type { FormContext } from '#/components/schema-form/context'
import { FieldErrors } from '#/components/schema-form/field-frame'
import { ObjectFields } from '#/components/schema-form/fields'
import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { Switch } from '#/components/ui/switch'
import type { JsonValue, Problem } from '#/lib/schema-form'
import { cn } from '#/lib/utils'
import {
	agentEnabled,
	agentField,
	agentKind,
	agentProviders,
	agentTypeLabel,
	type DependencyNote,
	isSignalKind,
	KIND_META,
	SIGNAL_PROVIDERS,
	settingsSchema,
} from '#/pages/profiles/lib/agents'
import { criteriaMatrix } from '#/pages/profiles/lib/matrix'
import { providerTiles } from '#/pages/profiles/lib/provider-tiles'
import {
	findProvider,
	notReadyProviders,
	providerName,
	providerStatusOf,
} from '#/pages/profiles/lib/search-agent'
import { belongsToPath } from '#/pages/profiles/lib/tabs'
import { AgentIdField } from '#/pages/profiles/ui/agent-id-field'
import { CriteriaMatrix } from '#/pages/profiles/ui/criteria-matrix'
import { DependencyNoteText } from '#/pages/profiles/ui/dependency-note'
import {
	ConfigureLink,
	ProviderLogo,
	ProviderTiles,
} from '#/pages/profiles/ui/provider-tiles'
import {
	type CriteriaActions,
	SearchCriteria,
} from '#/pages/profiles/ui/search-criteria'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { statusBorder } from '#/pages/source-providers/ui/provider-card'
import { StatusBadge } from '#/pages/source-providers/ui/status-badge'

export interface AgentPageActions extends CriteriaActions {
	onBack: () => void
	onName: (name: string) => void
	onId: (id: string) => void
	onEnabled: (enabled: boolean) => void
	onProvider: (key: string, attached: boolean) => void
	onOpenProvider: (key: string) => void
	onRequestRemove: () => void
	onConfirmRemove: () => void
	onCancelRemove: () => void
}

interface AgentPageProps {
	context: FormContext
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	note: DependencyNote | null
	catalog: readonly ProviderSummary[]
	countryQuery: string
	confirmingRemove: boolean
	actions: AgentPageActions
}

function exactly(problems: readonly Problem[], path: string): Problem[] {
	return problems.filter((problem) => problem.path === path)
}

const CARD = 'flex min-w-0 flex-col gap-4 rounded-xl border bg-card p-5'

export function AgentPage({
	context,
	agent,
	index,
	problems,
	note,
	catalog,
	countryQuery,
	confirmingRemove,
	actions,
}: AgentPageProps) {
	const base = `agents[${index}]`
	const kind = agentKind(agent)
	const name = agentField(agent, 'name')
	const own = problems.filter((problem) => belongsToPath(problem.path, base))
	const nameProblems = exactly(problems, `${base}.name`)
	const idProblems = exactly(problems, `${base}.id`)
	const attached = agentProviders(agent)
	const advancedOpen =
		idProblems.length > 0 ||
		exactly(problems, `${base}.accept_min_keywords`).length > 0
	const idField = (
		<AgentIdField
			id={`agent-id-${index}`}
			value={agentField(agent, 'id')}
			problems={idProblems}
			onChange={actions.onId}
		/>
	)

	return (
		<div data-agent-page={index} className="flex flex-col gap-5">
			<div>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					onClick={actions.onBack}
				>
					<ArrowLeftIcon />
					Agents de sourcing
				</Button>
			</div>
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div className="flex min-w-64 flex-1 flex-col gap-2">
					<div className="flex flex-wrap items-center gap-2">
						<Input
							aria-label="Nom de l'agent"
							value={name}
							aria-invalid={nameProblems.length > 0}
							className="h-10 max-w-md text-lg font-semibold"
							onChange={(event) => actions.onName(event.target.value)}
						/>
						{kind ? (
							<Badge variant="secondary">{agentTypeLabel(agent)}</Badge>
						) : null}
						{own.length > 0 ? (
							<Badge variant="destructive">
								{own.length} {own.length > 1 ? 'problèmes' : 'problème'}
							</Badge>
						) : null}
					</div>
					<FieldErrors id={`agent-name-${index}`} problems={nameProblems} />
					<p className="text-sm text-muted-foreground">
						{kind ? KIND_META[kind].description : 'Réglages de cet agent.'}
					</p>
				</div>
				<div className="flex flex-wrap items-center gap-3">
					<div className="flex items-center gap-2">
						<Switch
							id={`agent-enabled-${index}`}
							checked={agentEnabled(agent)}
							onCheckedChange={actions.onEnabled}
						/>
						<Label htmlFor={`agent-enabled-${index}`}>Activé</Label>
					</div>
					{confirmingRemove ? (
						<>
							<Button
								type="button"
								variant="destructive"
								size="sm"
								onClick={actions.onConfirmRemove}
							>
								Confirmer la suppression
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={actions.onCancelRemove}
							>
								Conserver
							</Button>
						</>
					) : (
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={actions.onRequestRemove}
						>
							<Trash2Icon />
							Supprimer
						</Button>
					)}
				</div>
			</div>
			{note ? <DependencyNoteText note={note} /> : null}
			<div className="grid gap-5 lg:grid-cols-5">
				<section
					aria-label={kind === 'search' ? 'Critères' : 'Réglages'}
					className={cn(CARD, 'lg:col-span-3')}
				>
					<h2 className="text-base font-semibold">
						{kind === 'search' ? 'Critères' : 'Réglages'}
					</h2>
					{kind === 'search' ? (
						<SearchCriteria
							context={context}
							agent={agent}
							index={index}
							idPrefix={`agent-${index}`}
							countryQuery={countryQuery}
							actions={actions}
							advancedOpen={advancedOpen}
							advancedExtra={idField}
						/>
					) : null}
					{isSignalKind(kind) ? (
						<>
							<ObjectFields
								context={context}
								schema={settingsSchema(kind)}
								path={['agents', index]}
								value={agent}
							/>
							<details
								open={idProblems.length > 0}
								className="rounded-md border px-3 py-2"
							>
								<summary className="cursor-pointer text-sm font-medium">
									Avancé
								</summary>
								<div className="mt-3">{idField}</div>
							</details>
						</>
					) : null}
				</section>
				<section
					aria-label={kind === 'search' ? 'Fournisseurs' : 'Fournisseur'}
					className={cn(CARD, 'lg:col-span-2')}
				>
					<h2 className="text-base font-semibold">
						{kind === 'search' ? 'Fournisseurs' : 'Fournisseur'}
					</h2>
					{kind === 'search' ? (
						<>
							<p className="text-xs text-muted-foreground">
								Cliquer sur une tuile pour l'ajouter ou la retirer. Chaque
								fournisseur traduit les mêmes critères dans son API.
							</p>
							<ProviderTiles
								tiles={providerTiles(catalog, attached)}
								label="Fournisseurs de l'agent"
								onToggle={actions.onProvider}
								onOpenProvider={actions.onOpenProvider}
							/>
							{notReadyProviders(attached, catalog).map((key) => (
								<p
									key={key}
									role="alert"
									className="rounded-md border border-amber-500/50 bg-amber-500/10 p-2 text-xs text-amber-700 dark:text-amber-400"
								>
									« {providerName(catalog, key)} » n'est pas prêt : l'agent
									l'ignorera tant qu'il n'est pas configuré et testé.
								</p>
							))}
							<FieldErrors
								id={`agent-${index}-providers`}
								problems={problems.filter((problem) =>
									belongsToPath(problem.path, `${base}.providers`),
								)}
							/>
						</>
					) : null}
					{isSignalKind(kind) ? (
						<SignalProvider
							providerKey={SIGNAL_PROVIDERS[kind]}
							catalog={catalog}
							onOpenProvider={actions.onOpenProvider}
						/>
					) : null}
				</section>
			</div>
			{kind === 'search' ? (
				<CriteriaMatrix columns={criteriaMatrix(catalog, attached)} />
			) : null}
		</div>
	)
}

interface SignalProviderProps {
	providerKey: string
	catalog: readonly ProviderSummary[]
	onOpenProvider: (key: string) => void
}

function SignalProvider({
	providerKey,
	catalog,
	onOpenProvider,
}: SignalProviderProps) {
	const name = providerName(catalog, providerKey)
	const status = providerStatusOf(catalog, providerKey)
	const provider = findProvider(catalog, providerKey)
	return (
		<div
			data-provider-tile={providerKey}
			data-status={status}
			className={cn(
				'flex flex-col items-center gap-2 rounded-xl border-2 p-4 text-center',
				statusBorder(status),
			)}
		>
			<ProviderLogo providerKey={providerKey} name={name} />
			<span className="text-sm font-medium">{name}</span>
			<StatusBadge status={status} />
			{provider ? (
				<span className="text-xs text-muted-foreground">
					{provider.description}
				</span>
			) : null}
			{status === 'ready' ? null : (
				<ConfigureLink
					providerKey={providerKey}
					name={name}
					onOpen={onOpenProvider}
				/>
			)}
		</div>
	)
}
