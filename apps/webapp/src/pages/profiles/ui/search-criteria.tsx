import type { ReactNode } from 'react'

import type { FormContext } from '#/components/schema-form/context'
import { ObjectFields } from '#/components/schema-form/fields'
import type { JsonValue } from '#/lib/schema-form'
import {
	advancedFieldsSchema,
	keywordFieldsSchema,
} from '#/pages/profiles/lib/agents'
import { withCountry, withoutCountry } from '#/pages/profiles/lib/countries'
import {
	agentContracts,
	agentCountries,
	agentRemote,
	allContracts,
	CONTRACT_OPTIONS,
	REMOTE_OPTIONS,
} from '#/pages/profiles/lib/search-agent'
import { CountryPicker } from '#/pages/profiles/ui/country-picker'

export interface CriteriaActions {
	onRemote: (remote: string) => void
	onContract: (contract: string, selected: boolean) => void
	onCountries: (codes: string[]) => void
	onCountryQuery: (query: string) => void
}

interface SearchCriteriaProps {
	context: FormContext
	agent: JsonValue
	index: number
	idPrefix: string
	countryQuery: string
	actions: CriteriaActions
	advancedOpen: boolean
	advancedExtra?: ReactNode
}

const SEGMENT =
	'cursor-pointer px-3 py-1.5 text-sm transition-colors not-last:border-r has-checked:bg-primary has-checked:text-primary-foreground has-focus-visible:ring-[3px] has-focus-visible:ring-ring/50'

const CHIP =
	'h-8 rounded-full border px-3 text-sm transition-colors aria-pressed:border-primary aria-pressed:bg-primary aria-pressed:text-primary-foreground hover:border-foreground/40'

export function SearchCriteria({
	context,
	agent,
	index,
	idPrefix,
	countryQuery,
	actions,
	advancedOpen,
	advancedExtra,
}: SearchCriteriaProps) {
	const remote = agentRemote(agent)
	const contracts = agentContracts(agent)
	const countries = agentCountries(agent)
	const everyContract = allContracts(agent)
	return (
		<div className="flex flex-col gap-5">
			<ObjectFields
				context={context}
				schema={keywordFieldsSchema()}
				path={['agents', index]}
				value={agent}
			/>
			<div className="flex flex-wrap gap-6">
				<div className="flex flex-col gap-1.5">
					<p id={`${idPrefix}-remote`} className="text-sm font-medium">
						Télétravail
					</p>
					<div
						role="radiogroup"
						aria-labelledby={`${idPrefix}-remote`}
						className="inline-flex w-fit overflow-hidden rounded-md border"
					>
						{REMOTE_OPTIONS.map((option) => (
							<label key={option.value} className={SEGMENT}>
								<input
									type="radio"
									name={`${idPrefix}-remote`}
									value={option.value}
									className="sr-only"
									checked={remote === option.value}
									onChange={() => actions.onRemote(option.value)}
								/>
								{option.label}
							</label>
						))}
					</div>
				</div>
				<div className="min-w-56 flex-1">
					<CountryPicker
						id={`${idPrefix}-countries`}
						codes={countries}
						query={countryQuery}
						onQueryChange={actions.onCountryQuery}
						onAdd={(code) => {
							actions.onCountries(withCountry(countries, code))
							actions.onCountryQuery('')
						}}
						onRemove={(code) =>
							actions.onCountries(withoutCountry(countries, code))
						}
					/>
				</div>
			</div>
			<fieldset className="flex flex-col gap-1.5">
				<legend className="mb-1.5 text-sm font-medium">Contrat</legend>
				<div className="flex flex-wrap gap-1.5">
					{CONTRACT_OPTIONS.map((option) => {
						const pressed =
							option.value === 'any'
								? everyContract
								: contracts.includes(option.value)
						return (
							<button
								key={option.value}
								type="button"
								aria-pressed={pressed}
								className={CHIP}
								onClick={() => actions.onContract(option.value, !pressed)}
							>
								{option.label}
							</button>
						)
					})}
				</div>
				<p className="text-xs text-muted-foreground">
					« Tous » ou aucun choix : aucun filtre de contrat.
				</p>
			</fieldset>
			<details open={advancedOpen} className="rounded-md border px-3 py-2">
				<summary className="cursor-pointer text-sm font-medium">Avancé</summary>
				<div className="mt-3 flex flex-col gap-4">
					<ObjectFields
						context={context}
						schema={advancedFieldsSchema()}
						path={['agents', index]}
						value={agent}
					/>
					{advancedExtra}
				</div>
			</details>
		</div>
	)
}
