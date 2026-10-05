import { PlusIcon } from 'lucide-react'

import type { FormContext } from '#/components/schema-form/context'
import { FieldErrors } from '#/components/schema-form/field-frame'
import { Button } from '#/components/ui/button'
import type { JsonValue, Problem } from '#/lib/schema-form'
import {
	type AgentKind,
	agentKind,
	dependencyNote,
} from '#/pages/profiles/lib/agents'
import { AgentCard } from '#/pages/profiles/ui/agent-card'
import { AgentPicker } from '#/pages/profiles/ui/agent-picker'

interface AgentsEditorProps {
	context: FormContext
	agents: readonly JsonValue[]
	problems: readonly Problem[]
	collapsed: ReadonlySet<number>
	confirmingRemove: number | null
	picking: boolean
	onOpenPicker: () => void
	onClosePicker: () => void
	onAdd: (kind: AgentKind) => void
	onToggleCollapsed: (index: number) => void
	onEnabled: (index: number, enabled: boolean) => void
	onName: (index: number, name: string) => void
	onId: (index: number, id: string) => void
	onCriteria: (index: number, criteria: string) => void
	onRequestRemove: (index: number) => void
	onConfirmRemove: (index: number) => void
	onCancelRemove: () => void
}

export function AgentsEditor({
	context,
	agents,
	problems,
	collapsed,
	confirmingRemove,
	picking,
	onOpenPicker,
	onClosePicker,
	onAdd,
	onToggleCollapsed,
	onEnabled,
	onName,
	onId,
	onCriteria,
	onRequestRemove,
	onConfirmRemove,
	onCancelRemove,
}: AgentsEditorProps) {
	return (
		<div className="flex flex-col gap-3">
			<FieldErrors
				id="agents"
				problems={problems.filter((problem) => problem.path === 'agents')}
			/>
			{agents.length === 0 ? (
				<p className="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
					No agent yet. Add one to start searching.
				</p>
			) : (
				<ul className="flex flex-col gap-3">
					{Object.keys(agents).map((slot) => {
						const index = Number(slot)
						const agent = agents[index]
						return (
							<AgentCard
								key={slot}
								context={context}
								agent={agent ?? null}
								index={index}
								problems={problems}
								note={dependencyNote(agentKind(agent), agents)}
								collapsed={collapsed.has(index)}
								confirmingRemove={confirmingRemove === index}
								onToggleCollapsed={() => onToggleCollapsed(index)}
								onEnabled={(enabled) => onEnabled(index, enabled)}
								onName={(name) => onName(index, name)}
								onId={(id) => onId(index, id)}
								onCriteria={(criteria) => onCriteria(index, criteria)}
								onRequestRemove={() => onRequestRemove(index)}
								onConfirmRemove={() => onConfirmRemove(index)}
								onCancelRemove={onCancelRemove}
							/>
						)
					})}
				</ul>
			)}
			{picking ? (
				<AgentPicker onPick={onAdd} onCancel={onClosePicker} />
			) : (
				<div>
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={onOpenPicker}
					>
						<PlusIcon />
						Add agent
					</Button>
				</div>
			)}
		</div>
	)
}
