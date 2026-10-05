import { ChevronDownIcon, ChevronRightIcon, Trash2Icon } from 'lucide-react'

import type { FormContext } from '#/components/schema-form/context'
import { FieldErrors } from '#/components/schema-form/field-frame'
import { ObjectFields } from '#/components/schema-form/fields'
import { Badge } from '#/components/ui/badge'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { Switch } from '#/components/ui/switch'
import { Textarea } from '#/components/ui/textarea'
import { isJsonObject, type JsonValue, type Problem } from '#/lib/schema-form'
import {
	agentCriteria,
	agentEnabled,
	agentField,
	agentKind,
	type DependencyNote,
	KIND_META,
	sourceSchema,
} from '#/pages/profiles/lib/agents'
import { belongsToPath } from '#/pages/profiles/lib/tabs'

interface AgentCardProps {
	context: FormContext
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	note: DependencyNote | null
	collapsed: boolean
	confirmingRemove: boolean
	onToggleCollapsed: () => void
	onEnabled: (enabled: boolean) => void
	onName: (name: string) => void
	onId: (id: string) => void
	onCriteria: (criteria: string) => void
	onRequestRemove: () => void
	onConfirmRemove: () => void
	onCancelRemove: () => void
}

function messages(problems: readonly Problem[], path: string): Problem[] {
	return problems.filter((problem) => problem.path === path)
}

export function AgentCard({
	context,
	agent,
	index,
	problems,
	note,
	collapsed,
	confirmingRemove,
	onToggleCollapsed,
	onEnabled,
	onName,
	onId,
	onCriteria,
	onRequestRemove,
	onConfirmRemove,
	onCancelRemove,
}: AgentCardProps) {
	const base = `agents[${index}]`
	const kind = agentKind(agent)
	const name = agentField(agent, 'name')
	const own = problems.filter((problem) => belongsToPath(problem.path, base))
	const nameId = `agent-name-${index}`
	const idId = `agent-id-${index}`
	const criteriaId = `agent-criteria-${index}`
	const nameProblems = messages(problems, `${base}.name`)
	const idProblems = messages(problems, `${base}.id`)
	const criteriaProblems = messages(problems, `${base}.llm.relevance_criteria`)
	const source = isJsonObject(agent) ? agent.source : undefined

	return (
		<li
			data-agent={index}
			className="flex flex-col gap-3 rounded-md border bg-muted/20 p-3"
		>
			<div className="flex flex-wrap items-center gap-2">
				<Button
					type="button"
					variant="ghost"
					size="icon"
					aria-expanded={!collapsed}
					aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${name || 'agent'}`}
					onClick={onToggleCollapsed}
				>
					{collapsed ? <ChevronRightIcon /> : <ChevronDownIcon />}
				</Button>
				<p className="text-sm font-medium">{name || 'Unnamed agent'}</p>
				{kind ? (
					<Badge variant="secondary">{KIND_META[kind].label}</Badge>
				) : null}
				{own.length > 0 ? (
					<Badge variant="destructive">{own.length} problems</Badge>
				) : null}
				<div className="ml-auto flex items-center gap-3">
					<span className="flex items-center gap-2 text-xs text-muted-foreground">
						<Switch
							aria-label={`Agent ${name || index + 1} enabled`}
							checked={agentEnabled(agent)}
							onCheckedChange={onEnabled}
						/>
						Enabled
					</span>
					{confirmingRemove ? (
						<span className="flex items-center gap-1">
							<Button
								type="button"
								variant="destructive"
								size="sm"
								onClick={onConfirmRemove}
							>
								Confirm remove
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={onCancelRemove}
							>
								Keep
							</Button>
						</span>
					) : (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							aria-label={`Remove agent ${name || index + 1}`}
							onClick={onRequestRemove}
						>
							<Trash2Icon />
							Remove
						</Button>
					)}
				</div>
			</div>
			{note ? (
				<p
					role={note.warn ? 'alert' : undefined}
					className={
						note.warn
							? 'rounded-md border border-amber-500/50 bg-amber-500/10 p-2 text-xs text-amber-700 dark:text-amber-400'
							: 'text-xs text-muted-foreground'
					}
				>
					{note.text}
					{note.warn ? ' No such agent in this profile yet.' : ''}
				</p>
			) : null}
			{collapsed ? null : (
				<div className="flex flex-col gap-4">
					<div className="grid gap-3 md:grid-cols-[2fr_1fr]">
						<div className="flex flex-col gap-1.5">
							<Label htmlFor={nameId}>Name</Label>
							<Input
								id={nameId}
								value={name}
								aria-invalid={nameProblems.length > 0}
								onChange={(event) => onName(event.target.value)}
							/>
							<FieldErrors id={nameId} problems={nameProblems} />
						</div>
						<div className="flex flex-col gap-1.5">
							<Label htmlFor={idId} className="text-xs text-muted-foreground">
								ID
							</Label>
							<Input
								id={idId}
								className="h-8 font-mono text-xs"
								value={agentField(agent, 'id')}
								aria-invalid={idProblems.length > 0}
								onChange={(event) => onId(event.target.value)}
							/>
							<FieldErrors id={idId} problems={idProblems} />
						</div>
					</div>
					{kind ? (
						<ObjectFields
							context={context}
							schema={sourceSchema(kind)}
							path={['agents', index, 'source']}
							value={source}
						/>
					) : null}
					{kind && KIND_META[kind].triaged ? (
						<div className="flex flex-col gap-1.5">
							<Label htmlFor={criteriaId}>LLM criteria for this agent</Label>
							<p className="text-xs text-muted-foreground">
								Used when this agent's candidates are triaged by the LLM. Falls
								back to the profile criteria.
							</p>
							<Textarea
								id={criteriaId}
								rows={3}
								aria-invalid={criteriaProblems.length > 0}
								value={agentCriteria(agent)}
								onChange={(event) => onCriteria(event.target.value)}
							/>
							<FieldErrors id={criteriaId} problems={criteriaProblems} />
						</div>
					) : null}
				</div>
			)}
		</li>
	)
}
