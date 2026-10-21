import { useState } from 'react'

import {
	isJsonObject,
	type JsonObject,
	type JsonValue,
} from '#/lib/schema-form'
import {
	nextStep,
	previousStep,
	stepProblems,
	type WizardStep,
} from '#/pages/profiles/lib/agent-wizard'
import {
	type AgentKind,
	agentField,
	agentList,
	newAgent,
	renameAgent,
	withAgents,
} from '#/pages/profiles/lib/agents'
import {
	withContract,
	withCountries,
	withFilter,
	withProvider,
} from '#/pages/profiles/lib/search-agent'

interface WizardState {
	step: WizardStep
	kind: AgentKind
	agent: JsonObject
	attempted: boolean
	countryQuery: string
}

export function useAgentWizard(
	value: JsonObject,
	onChange: (next: JsonObject) => void,
	onCreated: (id: string) => void,
) {
	const [state, setState] = useState<WizardState | null>(null)
	const agents = agentList(value)
	const index = agents.length

	const update = (change: (current: WizardState) => WizardState) =>
		setState((current) => (current === null ? null : change(current)))
	const edit = (change: (agent: JsonObject) => JsonObject) =>
		update((current) => ({ ...current, agent: change(current.agent) }))

	const preview =
		state === null ? value : withAgents(value, [...agents, state.agent])
	const problems = state?.attempted ? stepProblems(state.step, state.agent) : []

	return {
		state,
		index,
		preview,
		problems,
		open: () =>
			setState({
				step: 'type',
				kind: 'search',
				agent: newAgent('search', agents),
				attempted: false,
				countryQuery: '',
			}),
		close: () => setState(null),
		setPreview: (next: JsonValue) => {
			if (!isJsonObject(next)) return
			const candidate = agentList(next)[index]
			if (isJsonObject(candidate)) edit(() => candidate)
		},
		setKind: (kind: AgentKind) =>
			update((current) =>
				current.kind === kind
					? current
					: {
							...current,
							kind,
							agent: newAgent(kind, agents),
							attempted: false,
						},
			),
		setProvider: (key: string, attached: boolean) =>
			edit((agent) => withProvider(agent, key, attached)),
		setRemote: (remote: string) =>
			edit((agent) => withFilter(agent, 'remote', remote)),
		setContract: (contract: string, selected: boolean) =>
			edit((agent) => withContract(agent, contract, selected)),
		setCountries: (codes: string[]) =>
			edit((agent) => withCountries(agent, codes)),
		setCountryQuery: (countryQuery: string) =>
			update((current) => ({ ...current, countryQuery })),
		setName: (name: string) =>
			edit((agent) => {
				const renamed = renameAgent([...agents, agent], index, name)[index]
				return isJsonObject(renamed) ? renamed : agent
			}),
		next: () =>
			update((current) =>
				stepProblems(current.step, current.agent).length > 0
					? { ...current, attempted: true }
					: { ...current, step: nextStep(current.step), attempted: false },
			),
		back: () =>
			update((current) => ({
				...current,
				step: previousStep(current.step),
				attempted: false,
			})),
		create: () => {
			if (state === null) return
			if (stepProblems('summary', state.agent).length > 0) {
				update((current) => ({ ...current, attempted: true }))
				return
			}
			onChange(withAgents(value, [...agents, state.agent]))
			onCreated(agentField(state.agent, 'id'))
			setState(null)
		},
	}
}
