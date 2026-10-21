import { PlayIcon, Trash2Icon } from 'lucide-react'

import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Switch } from '#/components/ui/switch'
import type { JsonValue, Problem } from '#/lib/schema-form'
import { cn } from '#/lib/utils'
import { type AgentRunState, IDLE_RUN } from '#/pages/profiles/lib/agent-run'
import {
	agentEnabled,
	agentField,
	agentKind,
	agentProviders,
	agentSummary,
	agentTypeLabel,
	type DependencyNote,
} from '#/pages/profiles/lib/agents'
import {
	providerName,
	providerStatusOf,
} from '#/pages/profiles/lib/search-agent'
import { belongsToPath } from '#/pages/profiles/lib/tabs'
import { DependencyNoteText } from '#/pages/profiles/ui/dependency-note'
import { ProviderLogo } from '#/pages/profiles/ui/provider-tiles'
import { statusLabel } from '#/pages/source-providers/lib/labels'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { statusBorder } from '#/pages/source-providers/ui/provider-card'
import {
	agentListenerLine,
	isContinuousKind,
	listenerBadge,
} from '#/pages/sources/lib/listener'
import type { ListenerSummary } from '#/pages/sources/lib/types'

const NO_CATALOG: readonly ProviderSummary[] = []

interface AgentCardProps {
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	note: DependencyNote | null
	highlighted: boolean
	confirmingRemove: boolean
	catalog?: readonly ProviderSummary[]
	run?: AgentRunState
	onRunAgent?: () => void
	listener?: ListenerSummary
	now?: number
	onEdit: () => void
	onEnabled: (enabled: boolean) => void
	onRequestRemove: () => void
	onConfirmRemove: () => void
	onCancelRemove: () => void
}

function ProviderLogos({
	keys,
	catalog,
}: {
	keys: readonly string[]
	catalog: readonly ProviderSummary[]
}) {
	if (keys.length === 0) {
		return (
			<p className="text-xs text-muted-foreground">Aucune source choisie.</p>
		)
	}
	return (
		<ul aria-label="Sources de l'agent" className="flex flex-wrap gap-1.5">
			{keys.map((key) => {
				const name = providerName(catalog, key)
				const status = providerStatusOf(catalog, key)
				return (
					<li
						key={key}
						data-provider-logo={key}
						data-status={status}
						title={`${name} · ${statusLabel(status)}`}
						className={cn('rounded-xl border-2', statusBorder(status))}
					>
						<ProviderLogo providerKey={key} name={name} />
					</li>
				)
			})}
		</ul>
	)
}

export function AgentCard({
	agent,
	index,
	problems,
	note,
	highlighted,
	confirmingRemove,
	catalog = NO_CATALOG,
	run = IDLE_RUN,
	onRunAgent,
	listener,
	now = 0,
	onEdit,
	onEnabled,
	onRequestRemove,
	onConfirmRemove,
	onCancelRemove,
}: AgentCardProps) {
	const kind = agentKind(agent)
	const name = agentField(agent, 'name')
	const providers = agentProviders(agent)
	const own = problems.filter((problem) =>
		belongsToPath(problem.path, `agents[${index}]`),
	)
	const continuous = kind !== null && isContinuousKind(kind)

	return (
		<li
			data-agent={index}
			data-highlighted={highlighted}
			className={`flex h-full flex-col gap-2 rounded-md border bg-muted/20 p-3 ${
				highlighted ? 'ring-2 ring-primary' : ''
			}`}
		>
			<div className="flex items-start justify-between gap-2">
				<h3 className="text-sm font-medium">{name || 'Agent sans nom'}</h3>
				<Switch
					aria-label={`Agent ${name || index + 1} activé`}
					checked={agentEnabled(agent)}
					onCheckedChange={onEnabled}
				/>
			</div>
			<div className="flex flex-wrap items-center gap-2">
				{kind ? (
					<Badge variant="secondary">{agentTypeLabel(agent)}</Badge>
				) : null}
				{own.length > 0 ? (
					<Badge variant="destructive">
						{own.length} {own.length > 1 ? 'problèmes' : 'problème'}
					</Badge>
				) : null}
			</div>
			{kind === 'search' ? (
				<ProviderLogos keys={providers} catalog={catalog} />
			) : null}
			<p className="text-xs text-muted-foreground">{agentSummary(agent)}</p>
			{note ? <DependencyNoteText note={note} /> : null}
			<div className="mt-auto flex flex-wrap items-center gap-1 pt-1">
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
						{onRunAgent && kind && !continuous ? (
							<Button
								type="button"
								variant="secondary"
								size="sm"
								disabled={run.pending || providers.length === 0}
								aria-label={`Lancer l'agent ${name || index + 1}`}
								title="Lance maintenant chaque source de l'agent, pour tous les profils, avec la dernière version enregistrée"
								onClick={onRunAgent}
							>
								<PlayIcon />
								Lancer l'agent
							</Button>
						) : null}
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
			{run.message !== null ? (
				<output
					className={`text-xs ${run.error ? 'text-destructive' : 'text-muted-foreground'}`}
				>
					{run.message}
				</output>
			) : null}
			{continuous ? (
				<p
					data-listener-line
					data-tone={listenerBadge(listener?.state ?? '').tone}
					className="text-xs text-muted-foreground"
				>
					{agentListenerLine(listener, now)}
				</p>
			) : null}
		</li>
	)
}
