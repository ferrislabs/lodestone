import { Trash2Icon } from 'lucide-react'

import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Switch } from '#/components/ui/switch'
import type { JsonValue, Problem } from '#/lib/schema-form'
import {
	agentEnabled,
	agentField,
	agentKind,
	agentSummary,
	type DependencyNote,
	KIND_META,
} from '#/pages/profiles/lib/agents'
import { belongsToPath } from '#/pages/profiles/lib/tabs'
import { DependencyNoteText } from '#/pages/profiles/ui/agent-form'

interface AgentCardProps {
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	note: DependencyNote | null
	editing: boolean
	confirmingRemove: boolean
	onEdit: () => void
	onEnabled: (enabled: boolean) => void
	onRequestRemove: () => void
	onConfirmRemove: () => void
	onCancelRemove: () => void
}

export function AgentCard({
	agent,
	index,
	problems,
	note,
	editing,
	confirmingRemove,
	onEdit,
	onEnabled,
	onRequestRemove,
	onConfirmRemove,
	onCancelRemove,
}: AgentCardProps) {
	const kind = agentKind(agent)
	const name = agentField(agent, 'name')
	const own = problems.filter((problem) =>
		belongsToPath(problem.path, `agents[${index}]`),
	)

	return (
		<li
			data-agent={index}
			data-editing={editing}
			className={`flex h-full flex-col gap-2 rounded-md border bg-muted/20 p-3 ${
				editing ? 'ring-2 ring-primary' : ''
			}`}
		>
			<div className="flex items-start justify-between gap-2">
				<p className="text-sm font-medium">{name || 'Agent sans nom'}</p>
				<Switch
					aria-label={`Agent ${name || index + 1} activé`}
					checked={agentEnabled(agent)}
					onCheckedChange={onEnabled}
				/>
			</div>
			<div className="flex flex-wrap items-center gap-2">
				{kind ? (
					<Badge variant="secondary">{KIND_META[kind].label}</Badge>
				) : null}
				{own.length > 0 ? (
					<Badge variant="destructive">
						{own.length} {own.length > 1 ? 'problèmes' : 'problème'}
					</Badge>
				) : null}
			</div>
			<p className="text-xs text-muted-foreground">{agentSummary(agent)}</p>
			{note ? <DependencyNoteText note={note} /> : null}
			<div className="mt-auto flex items-center gap-1 pt-1">
				{confirmingRemove ? (
					<>
						<Button
							type="button"
							variant="destructive"
							size="sm"
							onClick={onConfirmRemove}
						>
							Confirmer la suppression
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={onCancelRemove}
						>
							Conserver
						</Button>
					</>
				) : (
					<>
						<Button
							type="button"
							variant="outline"
							size="sm"
							aria-label={`Modifier ${name || 'agent'}`}
							onClick={onEdit}
						>
							Modifier
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							aria-label={`Supprimer l'agent ${name || index + 1}`}
							onClick={onRequestRemove}
						>
							<Trash2Icon />
							Supprimer
						</Button>
					</>
				)}
			</div>
		</li>
	)
}
