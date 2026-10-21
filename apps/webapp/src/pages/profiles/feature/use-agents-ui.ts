import { useState } from 'react'

import {
	isJsonObject,
	type JsonObject,
	type JsonValue,
	setIn,
} from '#/lib/schema-form'
import {
	type AgentKind,
	agentList,
	newAgent,
	renameAgent,
	withAgents,
} from '#/pages/profiles/lib/agents'
import {
	searchProviders,
	withContract,
	withFilter,
	withProvider,
} from '#/pages/profiles/lib/search-agent'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'

function asObject(next: JsonValue, fallback: JsonObject): JsonObject {
	return isJsonObject(next) ? next : fallback
}

const NO_CATALOG: readonly ProviderSummary[] = []

export function useAgentsUi(
	value: JsonObject,
	onChange: (next: JsonObject) => void,
	catalog: readonly ProviderSummary[] = NO_CATALOG,
) {
	const [editing, setEditing] = useState<number | null>(null)
	const [confirmingRemove, setConfirmingRemove] = useState<number | null>(null)
	const [picking, setPicking] = useState(false)
	const agents = agentList(value)

	const patch = (path: (string | number)[], next: JsonValue) =>
		onChange(asObject(setIn(value, path, next), value))

	const update = (index: number, change: (agent: JsonObject) => JsonObject) =>
		onChange(
			withAgents(
				value,
				agents.map((agent, position) =>
					position === index && isJsonObject(agent) ? change(agent) : agent,
				),
			),
		)

	const firstReady = searchProviders(catalog).find(
		(provider) => provider.status === 'ready',
	)

	return {
		agents,
		editing,
		confirmingRemove,
		picking,
		openPicker: () => setPicking(true),
		closePicker: () => setPicking(false),
		add: (kind: AgentKind) => {
			onChange(
				withAgents(value, [
					...agents,
					newAgent(kind, agents, firstReady ? [firstReady.key] : []),
				]),
			)
			setPicking(false)
			setEditing(agents.length)
		},
		edit: setEditing,
		closeEdit: () => setEditing(null),
		setEnabled: (index: number, enabled: boolean) =>
			patch(['agents', index, 'enabled'], enabled),
		setName: (index: number, name: string) =>
			onChange(withAgents(value, renameAgent(agents, index, name))),
		setId: (index: number, id: string) => patch(['agents', index, 'id'], id),
		setProvider: (index: number, key: string, attached: boolean) =>
			update(index, (agent) => withProvider(agent, key, attached)),
		setRemote: (index: number, remote: string) =>
			update(index, (agent) => withFilter(agent, 'remote', remote)),
		setContract: (index: number, contract: string, selected: boolean) =>
			update(index, (agent) => withContract(agent, contract, selected)),
		requestRemove: setConfirmingRemove,
		cancelRemove: () => setConfirmingRemove(null),
		confirmRemove: (index: number) => {
			onChange(
				withAgents(
					value,
					agents.filter((_, position) => position !== index),
				),
			)
			setConfirmingRemove(null)
			setEditing((current) => {
				if (current === null || current === index) return null
				return current > index ? current - 1 : current
			})
		},
	}
}
