import { Trash2Icon } from 'lucide-react'

import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Switch } from '#/components/ui/switch'
import type { JsonValue, Problem } from '#/lib/schema-form'
import { agentEnabled, agentField } from '#/pages/profiles/lib/agents'
import {
	type ProviderOption,
	qualificationSummary,
} from '#/pages/profiles/lib/qualification'
import { belongsToPath } from '#/pages/profiles/lib/tabs'

interface QualificationCardProps {
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	providers: readonly ProviderOption[]
	editing: boolean
	confirmingRemove: boolean
	onEdit: () => void
	onEnabled: (enabled: boolean) => void
	onRequestRemove: () => void
	onConfirmRemove: () => void
	onCancelRemove: () => void
}

export function QualificationCard({
	agent,
	index,
	problems,
	providers,
	editing,
	confirmingRemove,
	onEdit,
	onEnabled,
	onRequestRemove,
	onConfirmRemove,
	onCancelRemove,
}: QualificationCardProps) {
	const name = agentField(agent, 'name')
	const own = problems.filter((problem) =>
		belongsToPath(problem.path, `qualification_agents[${index}]`),
	)
	return (
		<li
			data-qualification-agent={index}
			data-editing={editing}
			className={`flex h-full flex-col gap-2 rounded-md border bg-muted/20 p-3 ${
				editing ? 'ring-2 ring-primary' : ''
			}`}
		>
			<div className="flex items-start justify-between gap-2">
				<p className="text-sm font-medium">{name || 'Agent sans nom'}</p>
				<Switch
					aria-label={`Agent de qualification ${name || index + 1} activé`}
					checked={agentEnabled(agent)}
					onCheckedChange={onEnabled}
				/>
			</div>
			{own.length > 0 ? (
				<div className="flex flex-wrap items-center gap-2">
					<Badge variant="destructive">
						{own.length} {own.length > 1 ? 'problèmes' : 'problème'}
					</Badge>
				</div>
			) : null}
			<p className="text-xs text-muted-foreground">
				{qualificationSummary(agent, providers)}
			</p>
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
							aria-label={`Modifier ${name || "l'agent de qualification"}`}
							onClick={onEdit}
						>
							Modifier
						</Button>
						<Button
							type="button"
							variant="ghost"
							size="sm"
							aria-label={`Supprimer l'agent de qualification ${name || index + 1}`}
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
