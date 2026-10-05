import type { FormContext } from '#/components/schema-form/context'
import { FieldErrors } from '#/components/schema-form/field-frame'
import { ObjectFields } from '#/components/schema-form/fields'
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

interface AgentFormProps {
	context: FormContext
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	note: DependencyNote | null
	onEnabled: (enabled: boolean) => void
	onName: (name: string) => void
	onId: (id: string) => void
	onCriteria: (criteria: string) => void
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
	onEnabled,
	onName,
	onId,
	onCriteria,
}: AgentFormProps) {
	const base = `agents[${index}]`
	const kind = agentKind(agent)
	const nameId = `agent-name-${index}`
	const idId = `agent-id-${index}`
	const enabledId = `agent-enabled-${index}`
	const criteriaId = `agent-criteria-${index}`
	const nameProblems = messages(problems, `${base}.name`)
	const idProblems = messages(problems, `${base}.id`)
	const criteriaProblems = messages(problems, `${base}.llm.relevance_criteria`)
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
					<Label htmlFor={criteriaId}>Critères LLM propres à cet agent</Label>
					<p className="text-xs text-muted-foreground">
						Utilisés lorsque les candidats de cet agent sont triés par le LLM. À
						défaut, les critères du profil s'appliquent.
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
	)
}
