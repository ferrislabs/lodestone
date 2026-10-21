import type { FormContext } from '#/components/schema-form/context'
import { FieldErrors } from '#/components/schema-form/field-frame'
import { ObjectFields } from '#/components/schema-form/fields'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { Switch } from '#/components/ui/switch'
import { isJsonObject, type JsonValue, type Problem } from '#/lib/schema-form'
import {
	agentEnabled,
	agentField,
	agentKind,
	type DependencyNote,
	isSignalKind,
	sourceSchema,
} from '#/pages/profiles/lib/agents'
import {
	type SearchActions,
	SearchAgentFields,
} from '#/pages/profiles/ui/search-agent-fields'

interface AgentFormProps {
	context: FormContext
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	note: DependencyNote | null
	search: SearchActions
	onEnabled: (enabled: boolean) => void
	onName: (name: string) => void
	onId: (id: string) => void
}

function messages(problems: readonly Problem[], path: string): Problem[] {
	return problems.filter((problem) => problem.path === path)
}

export function DependencyNoteText({ note }: { note: DependencyNote }) {
	return (
		<p
			role={note.warn ? 'alert' : undefined}
			className={
				note.warn
					? 'rounded-md border border-amber-500/50 bg-amber-500/10 p-2 text-xs text-amber-700 dark:text-amber-400'
					: 'text-xs text-muted-foreground'
			}
		>
			{note.text}
			{note.warn
				? " Aucun agent de ce type dans ce profil pour l'instant."
				: ''}
		</p>
	)
}

export function AgentForm({
	context,
	agent,
	index,
	problems,
	note,
	search,
	onEnabled,
	onName,
	onId,
}: AgentFormProps) {
	const base = `agents[${index}]`
	const kind = agentKind(agent)
	const nameId = `agent-name-${index}`
	const idId = `agent-id-${index}`
	const enabledId = `agent-enabled-${index}`
	const nameProblems = messages(problems, `${base}.name`)
	const idProblems = messages(problems, `${base}.id`)
	const source = isJsonObject(agent) ? agent.source : undefined

	return (
		<div className="flex flex-col gap-5">
			{note ? <DependencyNoteText note={note} /> : null}
			<div className="flex items-end gap-4">
				<div className="flex flex-1 flex-col gap-1.5">
					<Label htmlFor={nameId}>Nom</Label>
					<Input
						id={nameId}
						value={agentField(agent, 'name')}
						aria-invalid={nameProblems.length > 0}
						onChange={(event) => onName(event.target.value)}
					/>
					<FieldErrors id={nameId} problems={nameProblems} />
				</div>
				<div className="flex h-9 items-center gap-2">
					<Switch
						id={enabledId}
						checked={agentEnabled(agent)}
						onCheckedChange={onEnabled}
					/>
					<Label htmlFor={enabledId}>Activé</Label>
				</div>
			</div>
			<details
				open={idProblems.length > 0}
				className="rounded-md border px-3 py-2"
			>
				<summary className="cursor-pointer text-sm font-medium">Avancé</summary>
				<div className="mt-3 flex flex-col gap-1.5">
					<Label htmlFor={idId}>Identifiant</Label>
					<p className="text-xs text-muted-foreground">
						Identifiant technique : minuscules, chiffres et tirets. Il suit le
						nom tant que vous ne le modifiez pas.
					</p>
					<Input
						id={idId}
						className="font-mono text-xs"
						value={agentField(agent, 'id')}
						aria-invalid={idProblems.length > 0}
						onChange={(event) => onId(event.target.value)}
					/>
					<FieldErrors id={idId} problems={idProblems} />
				</div>
			</details>
			{kind === 'search' ? (
				<SearchAgentFields
					context={context}
					agent={agent}
					index={index}
					problems={problems}
					actions={search}
				/>
			) : null}
			{isSignalKind(kind) ? (
				<ObjectFields
					context={context}
					schema={sourceSchema(kind)}
					path={['agents', index, 'source']}
					value={source}
				/>
			) : null}
		</div>
	)
}
