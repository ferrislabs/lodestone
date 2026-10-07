import type { FormContext } from '#/components/schema-form/context'
import { FieldErrors } from '#/components/schema-form/field-frame'
import { ObjectFields } from '#/components/schema-form/fields'
import { Button } from '#/components/ui/button'
import { isJsonObject, type JsonValue, type Problem } from '#/lib/schema-form'
import {
	agentProviders,
	filtersCountriesSchema,
	searchFieldsSchema,
} from '#/pages/profiles/lib/agents'
import {
	agentContracts,
	agentRemote,
	CONTRACT_OPTIONS,
	capabilityHint,
	capabilityText,
	findProvider,
	notReadyProviders,
	providerName,
	REMOTE_OPTIONS,
	searchProviders,
} from '#/pages/profiles/lib/search-agent'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { StatusBadge } from '#/pages/source-providers/ui/status-badge'

export interface SearchActions {
	catalog: readonly ProviderSummary[]
	onProvider: (index: number, key: string, attached: boolean) => void
	onRemote: (index: number, remote: string) => void
	onContract: (index: number, contract: string, selected: boolean) => void
	onOpenProvider: (key: string) => void
}

interface SearchAgentFieldsProps {
	context: FormContext
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	actions: SearchActions
}

function under(problems: readonly Problem[], path: string): Problem[] {
	return problems.filter(
		(problem) =>
			problem.path === path ||
			problem.path.startsWith(`${path}.`) ||
			problem.path.startsWith(`${path}[`),
	)
}

export function SearchAgentFields({
	context,
	agent,
	index,
	problems,
	actions,
}: SearchAgentFieldsProps) {
	const base = `agents[${index}]`
	const attached = agentProviders(agent)
	const options = searchProviders(actions.catalog)
	const unready = notReadyProviders(attached, actions.catalog)
	const contracts = agentContracts(agent)
	const remote = agentRemote(agent)

	return (
		<div className="flex flex-col gap-5">
			<fieldset className="flex flex-col gap-2">
				<legend className="text-sm font-medium">Fournisseurs</legend>
				<p className="text-xs text-muted-foreground">
					Chaque fournisseur traduit les mêmes critères dans son API.
				</p>
				<ul className="flex flex-col gap-1.5">
					{options.map((provider) => {
						const selected = attached.includes(provider.key)
						const ready = provider.status === 'ready'
						const id = `agent-${index}-provider-${provider.key}`
						return (
							<li
								key={provider.key}
								data-provider-option={provider.key}
								className="flex flex-wrap items-center gap-2 rounded-md border px-3 py-2"
							>
								<input
									id={id}
									type="checkbox"
									checked={selected}
									disabled={!ready && !selected}
									onChange={(event) =>
										actions.onProvider(
											index,
											provider.key,
											event.target.checked,
										)
									}
								/>
								<label htmlFor={id} className="text-sm">
									{provider.name}
								</label>
								<StatusBadge status={provider.status} />
								{ready ? null : (
									<>
										<span className="text-xs text-muted-foreground">
											à configurer
										</span>
										<Button
											type="button"
											variant="link"
											size="sm"
											aria-label={`Configurer ${provider.name}`}
											onClick={() => actions.onOpenProvider(provider.key)}
										>
											Ouvrir le catalogue
										</Button>
									</>
								)}
							</li>
						)
					})}
				</ul>
				{unready.map((key) => (
					<p
						key={key}
						role="alert"
						className="rounded-md border border-amber-500/50 bg-amber-500/10 p-2 text-xs text-amber-700 dark:text-amber-400"
					>
						« {providerName(actions.catalog, key)} » n'est pas prêt : l'agent
						l'ignorera tant qu'il n'est pas configuré et testé.
					</p>
				))}
				<FieldErrors
					id={`agent-${index}-providers`}
					problems={under(problems, `${base}.providers`)}
				/>
			</fieldset>
			<ObjectFields
				context={context}
				schema={searchFieldsSchema()}
				path={['agents', index]}
				value={agent}
			/>
			<fieldset className="flex flex-col gap-4 rounded-md border p-3">
				<legend className="px-1 text-sm font-medium">Filtres</legend>
				<div
					role="radiogroup"
					aria-label="Télétravail"
					className="flex flex-col gap-1.5"
				>
					<p className="text-sm font-medium">Télétravail</p>
					<div className="flex flex-wrap gap-4">
						{REMOTE_OPTIONS.map((option) => {
							const id = `agent-${index}-remote-${option.value}`
							return (
								<span key={option.value} className="flex items-center gap-1.5">
									<input
										id={id}
										type="radio"
										name={`agent-${index}-remote`}
										checked={remote === option.value}
										onChange={() => actions.onRemote(index, option.value)}
									/>
									<label htmlFor={id} className="text-sm">
										{option.label}
									</label>
								</span>
							)
						})}
					</div>
				</div>
				<ObjectFields
					context={context}
					schema={filtersCountriesSchema()}
					path={['agents', index, 'filters']}
					value={isJsonObject(agent) ? agent.filters : undefined}
				/>
				<div className="flex flex-col gap-1.5">
					<p className="text-sm font-medium">Contrat</p>
					<div className="flex flex-wrap gap-4">
						{CONTRACT_OPTIONS.map((option) => {
							const id = `agent-${index}-contract-${option.value}`
							return (
								<span key={option.value} className="flex items-center gap-1.5">
									<input
										id={id}
										type="checkbox"
										checked={contracts.includes(option.value)}
										onChange={(event) =>
											actions.onContract(
												index,
												option.value,
												event.target.checked,
											)
										}
									/>
									<label htmlFor={id} className="text-sm">
										{option.label}
									</label>
								</span>
							)
						})}
					</div>
					<p className="text-xs text-muted-foreground">
						Sans choix, ou avec « Tous les contrats », aucun filtre de contrat.
					</p>
				</div>
				<div className="flex flex-col gap-1" data-capability-hints>
					<p className="text-xs font-medium uppercase text-muted-foreground">
						Prise en charge par fournisseur
					</p>
					{attached.length === 0 ? (
						<p className="text-xs text-muted-foreground">
							Choisir un fournisseur pour voir ce qu'il filtre.
						</p>
					) : (
						<ul className="flex flex-col gap-1">
							{attached.map((key) => {
								const provider = findProvider(actions.catalog, key)
								return (
									<li key={key} data-hint={key} className="text-xs">
										<span className="font-medium">
											{providerName(actions.catalog, key)}
										</span>{' '}
										:{' '}
										{provider
											? capabilityText(capabilityHint(provider))
											: 'fournisseur absent du catalogue'}
									</li>
								)
							})}
						</ul>
					)}
				</div>
			</fieldset>
		</div>
	)
}
