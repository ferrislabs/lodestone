import { PlusIcon } from 'lucide-react'

import { FieldErrors } from '#/components/schema-form/field-frame'
import type { JsonValue, Problem } from '#/lib/schema-form'
import { agentRunState, runAgent } from '#/pages/profiles/lib/agent-run'
import {
	agentField,
	agentKind,
	agentProviders,
	dependencyNote,
	SIGNAL_PROVIDERS,
} from '#/pages/profiles/lib/agents'
import { AgentCard } from '#/pages/profiles/ui/agent-card'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import type { SourceTrigger } from '#/pages/sources/feature/use-source-trigger'
import type { ListenerSummary } from '#/pages/sources/lib/types'

interface AgentsEditorProps {
	agents: readonly JsonValue[]
	problems: readonly Problem[]
	highlighted: string | null
	confirmingRemove: number | null
	catalog: readonly ProviderSummary[]
	sourceTrigger?: SourceTrigger
	listeners?: Readonly<Record<string, ListenerSummary>>
	now?: number
	onAdd: () => void
	onEdit: (index: number) => void
	onEnabled: (index: number, enabled: boolean) => void
	onRequestRemove: (index: number) => void
	onConfirmRemove: (index: number) => void
	onCancelRemove: () => void
}

export function AgentsEditor({
	agents,
	problems,
	highlighted,
	confirmingRemove,
	catalog,
	sourceTrigger,
	listeners,
	now,
	onAdd,
	onEdit,
	onEnabled,
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
					const kind = agentKind(agent)
					const providers = agentProviders(agent)
					return (
						<AgentCard
							key={slot}
							agent={agent ?? null}
							index={index}
							problems={problems}
							note={dependencyNote(agent, agents)}
							highlighted={
								highlighted !== null && agentField(agent, 'id') === highlighted
							}
							confirmingRemove={confirmingRemove === index}
							catalog={catalog}
							run={
								sourceTrigger
									? agentRunState(
											providers,
											sourceTrigger.pending,
											sourceTrigger.results,
											catalog,
										)
									: undefined
							}
							onRunAgent={
								sourceTrigger
									? () => runAgent(providers, sourceTrigger.trigger)
									: undefined
							}
							listener={
								kind && kind !== 'search'
									? listeners?.[SIGNAL_PROVIDERS[kind]]
									: undefined
							}
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
						onClick={onAdd}
					>
						<PlusIcon className="size-5" />
						Ajouter un agent
					</button>
				</li>
			</ul>
		</div>
	)
}
