import { useState } from 'react'

import {
	isJsonObject,
	type JsonObject,
	type JsonValue,
	removeIn,
	setIn,
} from '#/lib/schema-form'
import {
	type AgentKind,
	agentList,
	newAgent,
	renameAgent,
	withAgents,
} from '#/pages/profiles/lib/agents'

function asObject(next: JsonValue, fallback: JsonObject): JsonObject {
	return isJsonObject(next) ? next : fallback
}

export function useAgentsUi(
	value: JsonObject,
	onChange: (next: JsonObject) => void,
) {
	const [editing, setEditing] = useState<number | null>(null)
	const [confirmingRemove, setConfirmingRemove] = useState<number | null>(null)
	const [picking, setPicking] = useState(false)
	const agents = agentList(value)

	const patch = (path: (string | number)[], next: JsonValue) =>
		onChange(asObject(setIn(value, path, next), value))

	return {
		agents,
		editing,
		confirmingRemove,
		picking,
		openPicker: () => setPicking(true),
		closePicker: () => setPicking(false),
		add: (kind: AgentKind) => {
			onChange(withAgents(value, [...agents, newAgent(kind, agents)]))
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
		setCriteria: (index: number, criteria: string) => {
			if (criteria === '') {
				onChange(asObject(removeIn(value, ['agents', index, 'llm']), value))
			} else {
				patch(['agents', index, 'llm'], { relevance_criteria: criteria })
			}
		},
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
