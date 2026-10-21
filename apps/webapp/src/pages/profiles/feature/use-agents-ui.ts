import { useRef, useState } from 'react'

import {
	isJsonObject,
	type JsonObject,
	type JsonValue,
	setIn,
} from '#/lib/schema-form'
import {
	agentField,
	agentList,
	renameAgent,
	withAgents,
} from '#/pages/profiles/lib/agents'
import {
	withContract,
	withCountries,
	withFilter,
	withProvider,
} from '#/pages/profiles/lib/search-agent'

function asObject(next: JsonValue, fallback: JsonObject): JsonObject {
	return isJsonObject(next) ? next : fallback
}

export type OpenAgent = (id: string | null, replace: boolean) => void

function idAt(agents: readonly JsonValue[], index: number): string | null {
	return index >= 0 && index < agents.length
		? agentField(agents[index], 'id')
		: null
}

interface Remembered {
	index: number
	id: string
	url: string | null
}

function useOpenIndex(
	agents: readonly JsonValue[],
	agentId: string | null,
): { index: number; expect: (index: number, id: string) => void } {
	const last = useRef<Remembered | null>(null)
	const remembered = last.current
	let index = -1
	if (agentId !== null) {
		if (remembered !== null && idAt(agents, remembered.index) === agentId) {
			index = remembered.index
		} else if (
			remembered !== null &&
			remembered.url === agentId &&
			idAt(agents, remembered.index) === remembered.id
		) {
			index = remembered.index
		} else {
			index = agents.findIndex((agent) => agentField(agent, 'id') === agentId)
		}
	}
	last.current =
		index >= 0 ? { index, id: idAt(agents, index) ?? '', url: agentId } : null
	return {
		index,
		expect: (position, id) => {
			last.current = { index: position, id, url: agentId }
		},
	}
}

export function useAgentsUi(
	value: JsonObject,
	onChange: (next: JsonObject) => void,
	agentId: string | null,
	openAgent: OpenAgent,
) {
	const [confirmingRemove, setConfirmingRemove] = useState<number | null>(null)
	const [highlighted, setHighlighted] = useState<string | null>(null)
	const [countryQuery, setCountryQuery] = useState('')
	const agents = agentList(value)
	const open = useOpenIndex(agents, agentId)

	const update = (index: number, change: (agent: JsonObject) => JsonObject) =>
		onChange(
			withAgents(
				value,
				agents.map((agent, position) =>
					position === index && isJsonObject(agent) ? change(agent) : agent,
				),
			),
		)

	const follow = (index: number, next: readonly JsonValue[]) => {
		const id = idAt(next, index)
		if (id === null || index !== open.index) return
		open.expect(index, id)
		if (id !== agentId) openAgent(id, true)
	}

	const remove = (index: number) => {
		onChange(
			withAgents(
				value,
				agents.filter((_, position) => position !== index),
			),
		)
		setConfirmingRemove(null)
		if (index === open.index) openAgent(null, false)
	}

	return {
		agents,
		openIndex: open.index,
		confirmingRemove,
		highlighted,
		countryQuery,
		setHighlighted,
		edit: (index: number) => {
			setCountryQuery('')
			setConfirmingRemove(null)
			const id = idAt(agents, index)
			if (id === null) return
			open.expect(index, id)
			openAgent(id, false)
		},
		back: () => {
			setConfirmingRemove(null)
			const id = idAt(agents, open.index)
			if (id !== null) setHighlighted(id)
			openAgent(null, false)
		},
		setEnabled: (index: number, enabled: boolean) =>
			onChange(
				asObject(setIn(value, ['agents', index, 'enabled'], enabled), value),
			),
		setName: (index: number, name: string) => {
			const next = renameAgent(agents, index, name)
			onChange(withAgents(value, next))
			follow(index, next)
		},
		setId: (index: number, id: string) => {
			const next = agents.map((agent, position) =>
				position === index && isJsonObject(agent) ? { ...agent, id } : agent,
			)
			onChange(withAgents(value, next))
			follow(index, next)
		},
		setProvider: (index: number, key: string, attached: boolean) =>
			update(index, (agent) => withProvider(agent, key, attached)),
		setRemote: (index: number, remote: string) =>
			update(index, (agent) => withFilter(agent, 'remote', remote)),
		setContract: (index: number, contract: string, selected: boolean) =>
			update(index, (agent) => withContract(agent, contract, selected)),
		setCountries: (index: number, codes: string[]) =>
			update(index, (agent) => withCountries(agent, codes)),
		setCountryQuery,
		requestRemove: setConfirmingRemove,
		cancelRemove: () => setConfirmingRemove(null),
		confirmRemove: remove,
	}
}
