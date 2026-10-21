import { CheckIcon } from 'lucide-react'

import type { FormContext } from '#/components/schema-form/context'
import { ObjectFields } from '#/components/schema-form/fields'
import { Button } from '#/components/ui/button'
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from '#/components/ui/dialog'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import type { JsonObject } from '#/lib/schema-form'
import { cn } from '#/lib/utils'
import {
	WIZARD_STEPS,
	type WizardStep,
} from '#/pages/profiles/lib/agent-wizard'
import {
	AGENT_KINDS,
	type AgentKind,
	agentField,
	agentProviders,
	agentSummary,
	isSignalKind,
	KIND_META,
	settingsSchema,
} from '#/pages/profiles/lib/agents'
import { countryName } from '#/pages/profiles/lib/countries'
import { criteriaMatrix } from '#/pages/profiles/lib/matrix'
import { providerTiles } from '#/pages/profiles/lib/provider-tiles'
import {
	agentContracts,
	agentCountries,
	agentRemote,
	allContracts,
	CONTRACT_OPTIONS,
	providerName,
	REMOTE_OPTIONS,
} from '#/pages/profiles/lib/search-agent'
import { CriteriaMatrix } from '#/pages/profiles/ui/criteria-matrix'
import { ProviderTiles } from '#/pages/profiles/ui/provider-tiles'
import {
	type CriteriaActions,
	SearchCriteria,
} from '#/pages/profiles/ui/search-criteria'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'

export interface WizardActions extends CriteriaActions {
	onKind: (kind: AgentKind) => void
	onProvider: (key: string, attached: boolean) => void
	onOpenProvider: (key: string) => void
	onName: (name: string) => void
	onNext: () => void
	onBack: () => void
	onCreate: () => void
	onClose: () => void
}

interface AgentWizardProps {
	step: WizardStep
	kind: AgentKind
	agent: JsonObject
	index: number
	context: FormContext
	catalog: readonly ProviderSummary[]
	countryQuery: string
	problems: readonly string[]
	actions: WizardActions
}

function stringList(agent: JsonObject, key: string): string[] {
	const items = agent[key]
	return Array.isArray(items)
		? items.filter((item): item is string => typeof item === 'string')
		: []
}

function labelOf(
	options: readonly { value: string; label: string }[],
	value: string,
): string {
	return options.find((option) => option.value === value)?.label ?? value
}

function StepList({ step }: { step: WizardStep }) {
	const current = WIZARD_STEPS.findIndex((item) => item.id === step)
	return (
		<ol aria-label="Étapes" className="flex flex-wrap items-center gap-3">
			{WIZARD_STEPS.map((item, position) => {
				const done = position < current
				return (
					<li
						key={item.id}
						aria-current={item.id === step ? 'step' : undefined}
						className={cn(
							'flex items-center gap-2 text-sm text-muted-foreground',
							item.id === step && 'font-semibold text-foreground',
						)}
					>
						<span
							className={cn(
								'flex size-6 items-center justify-center rounded-full border text-xs',
								done && 'border-emerald-600 bg-emerald-600 text-white',
								item.id === step &&
									'border-primary bg-primary text-primary-foreground',
							)}
						>
							{done ? <CheckIcon className="size-3.5" /> : position + 1}
						</span>
						{item.label}
					</li>
				)
			})}
		</ol>
	)
}

function TypeStep({
	kind,
	agent,
	catalog,
	actions,
}: Pick<AgentWizardProps, 'kind' | 'agent' | 'catalog' | 'actions'>) {
	return (
		<div className="flex flex-col gap-5">
			<div
				role="radiogroup"
				aria-label="Type d'agent"
				className="grid gap-2 md:grid-cols-3"
			>
				{AGENT_KINDS.map((option) => (
					<label
						key={option}
						className="flex h-full cursor-pointer flex-col gap-0.5 rounded-md border bg-background p-3 text-left hover:border-primary has-checked:border-primary has-checked:ring-2 has-checked:ring-primary has-focus-visible:ring-[3px] has-focus-visible:ring-ring/50"
					>
						<input
							type="radio"
							name="agent-wizard-kind"
							value={option}
							className="sr-only"
							checked={kind === option}
							onChange={() => actions.onKind(option)}
						/>
						<span className="text-sm font-medium">
							{KIND_META[option].label}
						</span>
						<span className="text-xs text-muted-foreground">
							{KIND_META[option].description}
						</span>
					</label>
				))}
			</div>
			{kind === 'search' ? (
				<div className="flex flex-col gap-2">
					<p className="text-sm font-medium">Fournisseurs</p>
					<p className="text-xs text-muted-foreground">
						Cliquer sur une tuile pour l'ajouter ou la retirer. Un fournisseur à
						configurer se règle dans le catalogue.
					</p>
					<ProviderTiles
						tiles={providerTiles(catalog, agentProviders(agent))}
						label="Fournisseurs de l'agent"
						onToggle={actions.onProvider}
						onOpenProvider={actions.onOpenProvider}
					/>
				</div>
			) : null}
		</div>
	)
}

function SummaryStep({
	agent,
	kind,
	catalog,
	actions,
}: Pick<AgentWizardProps, 'kind' | 'agent' | 'catalog' | 'actions'>) {
	const providers = agentProviders(agent)
	const rows: { label: string; value: string }[] =
		kind === 'search'
			? [
					{ label: 'Type', value: KIND_META.search.label },
					{
						label: 'Fournisseurs',
						value: providers
							.map((key) => providerName(catalog, key))
							.join(', '),
					},
					{
						label: 'Mots-clés',
						value: stringList(agent, 'keywords').join(', ') || 'aucun',
					},
					{
						label: 'Exclusions',
						value: stringList(agent, 'exclusions').join(', ') || 'aucune',
					},
					{
						label: 'Télétravail',
						value: labelOf(REMOTE_OPTIONS, agentRemote(agent)),
					},
					{
						label: 'Pays',
						value:
							agentCountries(agent).map(countryName).join(', ') ||
							'tous les pays',
					},
					{
						label: 'Contrat',
						value: allContracts(agent)
							? 'tous les contrats'
							: agentContracts(agent)
									.map((value) => labelOf(CONTRACT_OPTIONS, value))
									.join(', '),
					},
				]
			: [
					{ label: 'Type', value: KIND_META[kind].label },
					{ label: 'Réglages', value: agentSummary(agent) },
				]
	return (
		<div className="flex flex-col gap-5">
			<div className="flex flex-col gap-1.5">
				<Label htmlFor="agent-wizard-name">Nom</Label>
				<Input
					id="agent-wizard-name"
					value={agentField(agent, 'name')}
					onChange={(event) => actions.onName(event.target.value)}
				/>
			</div>
			<dl
				aria-label="Récapitulatif"
				className="grid gap-x-4 gap-y-2 text-sm sm:grid-cols-[10rem_1fr]"
			>
				{rows.map((row) => (
					<div key={row.label} className="contents">
						<dt className="text-muted-foreground">{row.label}</dt>
						<dd>{row.value}</dd>
					</div>
				))}
			</dl>
			{kind === 'search' ? (
				<CriteriaMatrix columns={criteriaMatrix(catalog, providers)} />
			) : null}
		</div>
	)
}

export function AgentWizard({
	step,
	kind,
	agent,
	index,
	context,
	catalog,
	countryQuery,
	problems,
	actions,
}: AgentWizardProps) {
	const first = step === WIZARD_STEPS[0]?.id
	const last = step === WIZARD_STEPS[WIZARD_STEPS.length - 1]?.id
	return (
		<Dialog
			open
			onOpenChange={(open) => {
				if (!open) actions.onClose()
			}}
		>
			<DialogContent className="max-h-[90vh] gap-0 overflow-hidden p-0 sm:max-w-4xl">
				<DialogHeader className="gap-3 border-b px-6 py-4 pr-12">
					<DialogTitle>Nouvel agent de sourcing</DialogTitle>
					<DialogDescription className="sr-only">
						Assistant de création en trois étapes.
					</DialogDescription>
					<StepList step={step} />
				</DialogHeader>
				<div className="flex max-h-[60vh] flex-col gap-4 overflow-y-auto px-6 py-5">
					{step === 'type' ? (
						<TypeStep
							kind={kind}
							agent={agent}
							catalog={catalog}
							actions={actions}
						/>
					) : null}
					{step === 'criteria' && kind === 'search' ? (
						<SearchCriteria
							context={context}
							agent={agent}
							index={index}
							idPrefix="agent-wizard"
							countryQuery={countryQuery}
							actions={actions}
							advancedOpen={false}
						/>
					) : null}
					{step === 'criteria' && isSignalKind(kind) ? (
						<ObjectFields
							context={context}
							schema={settingsSchema(kind)}
							path={['agents', index]}
							value={agent}
						/>
					) : null}
					{step === 'summary' ? (
						<SummaryStep
							kind={kind}
							agent={agent}
							catalog={catalog}
							actions={actions}
						/>
					) : null}
					{problems.length > 0 ? (
						<ul className="flex flex-col gap-1">
							{problems.map((problem) => (
								<li
									key={problem}
									role="alert"
									className="text-sm text-destructive"
								>
									{problem}
								</li>
							))}
						</ul>
					) : null}
				</div>
				<DialogFooter className="border-t px-6 py-4 sm:justify-between">
					<Button
						type="button"
						variant="outline"
						onClick={first ? actions.onClose : actions.onBack}
					>
						{first ? 'Annuler' : 'Précédent'}
					</Button>
					{last ? (
						<Button type="button" onClick={actions.onCreate}>
							Créer
						</Button>
					) : (
						<Button type="button" onClick={actions.onNext}>
							Suivant
						</Button>
					)}
				</DialogFooter>
			</DialogContent>
		</Dialog>
	)
}
