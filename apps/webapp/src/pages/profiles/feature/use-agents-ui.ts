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

function focusName(index: number) {
	requestAnimationFrame(() => {
		document.getElementById(`agent-name-${index}`)?.focus()
	})
}

export function useAgentsUi(
	value: JsonObject,
	onChange: (next: JsonObject) => void,
) {
	const [collapsed, setCollapsed] = useState<ReadonlySet<number>>(
		() => new Set(agentList(value).map((_, index) => index)),
	)
	const [confirmingRemove, setConfirmingRemove] = useState<number | null>(null)
	const [picking, setPicking] = useState(false)
	const agents = agentList(value)

	const patch = (path: (string | number)[], next: JsonValue) =>
		onChange(asObject(setIn(value, path, next), value))

	return {
		agents,
		collapsed,
		confirmingRemove,
		picking,
		openPicker: () => setPicking(true),
		closePicker: () => setPicking(false),
		add: (kind: AgentKind) => {
			onChange(withAgents(value, [...agents, newAgent(kind, agents)]))
			setPicking(false)
			focusName(agents.length)
		},
		toggleCollapsed: (index: number) =>
			setCollapsed((current) => {
				const next = new Set(current)
				if (!next.delete(index)) next.add(index)
				return next
			}),
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
			setCollapsed(
				(current) =>
					new Set(
						[...current]
							.filter((position) => position !== index)
							.map((position) => (position > index ? position - 1 : position)),
					),
			)
		},
	}
}
