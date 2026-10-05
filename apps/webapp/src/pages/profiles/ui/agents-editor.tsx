import { PlusIcon } from 'lucide-react'

import type { FormContext } from '#/components/schema-form/context'
import { FieldErrors } from '#/components/schema-form/field-frame'
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
					Aucun agent pour le moment. Ajouter un agent pour démarrer la
					recherche.
				</p>
			) : null}
			<ul className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
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
				<li data-add-agent>
					<button
						type="button"
						className="flex h-full min-h-32 w-full flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed text-sm text-muted-foreground hover:border-primary hover:text-foreground"
						onClick={onOpenPicker}
					>
						<PlusIcon className="size-5" />
						Ajouter un agent
					</button>
				</li>
			</ul>
			{picking ? <AgentPicker onPick={onAdd} onCancel={onClosePicker} /> : null}
		</div>
	)
}
