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
import { AgentSheet } from '#/pages/profiles/ui/agent-sheet'
import type { SourceTrigger } from '#/pages/sources/feature/use-source-trigger'
import type { ListenerSummary } from '#/pages/sources/lib/types'

interface AgentsEditorProps {
	context: FormContext
	agents: readonly JsonValue[]
	problems: readonly Problem[]
	editing: number | null
	confirmingRemove: number | null
	picking: boolean
	sourceTrigger?: SourceTrigger
	listeners?: Readonly<Record<string, ListenerSummary>>
	now?: number
	onOpenPicker: () => void
	onClosePicker: () => void
	onAdd: (kind: AgentKind) => void
	onEdit: (index: number) => void
	onCloseEdit: () => void
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
	editing,
	confirmingRemove,
	picking,
	sourceTrigger,
	listeners,
	now,
	onOpenPicker,
	onClosePicker,
	onAdd,
	onEdit,
	onCloseEdit,
	onEnabled,
	onName,
	onId,
	onCriteria,
	onRequestRemove,
	onConfirmRemove,
	onCancelRemove,
}: AgentsEditorProps) {
	const editingAgent = editing === null ? undefined : agents[editing]
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
					const kind = agentKind(agent)
					return (
						<AgentCard
							key={slot}
							agent={agent ?? null}
							index={index}
							problems={problems}
							note={dependencyNote(agentKind(agent), agents)}
							editing={editing === index}
							confirmingRemove={confirmingRemove === index}
							runPending={
								kind !== null && sourceTrigger?.pending.has(kind) === true
							}
							runResult={
								kind === null ? null : (sourceTrigger?.results[kind] ?? null)
							}
							onRunSource={
								sourceTrigger && kind
									? () => sourceTrigger.trigger(kind)
									: undefined
							}
							listener={kind ? listeners?.[kind] : undefined}
							now={now}
							onEdit={() => onEdit(index)}
							onEnabled={(enabled) => onEnabled(index, enabled)}
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
			{editingAgent !== undefined && editing !== null ? (
				<AgentSheet
					key={editing}
					context={context}
					agent={editingAgent}
					index={editing}
					problems={problems}
					note={dependencyNote(agentKind(editingAgent), agents)}
					onClose={onCloseEdit}
					onEnabled={(enabled) => onEnabled(editing, enabled)}
					onName={(name) => onName(editing, name)}
					onId={(id) => onId(editing, id)}
					onCriteria={(criteria) => onCriteria(editing, criteria)}
				/>
			) : null}
		</div>
	)
}
