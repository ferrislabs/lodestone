import { PlusIcon } from 'lucide-react'

import { FieldErrors } from '#/components/schema-form/field-frame'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { Switch } from '#/components/ui/switch'
import { Textarea } from '#/components/ui/textarea'
import type { JsonValue, Problem } from '#/lib/schema-form'
import { agentEnabled, agentField } from '#/pages/profiles/lib/agents'
import {
	agentProvider,
	agentWeight,
	contractSchema,
	MAX_PROMPT_CHARS,
	type ProviderOption,
	questionsOf,
} from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'
import { ContractPreview } from '#/pages/profiles/ui/contract-preview'
import { ProviderSelect } from '#/pages/profiles/ui/provider-select'
import { QuestionCard } from '#/pages/profiles/ui/question-card'
import { WeightPicker } from '#/pages/profiles/ui/weight-picker'

interface QualificationFormProps {
	agent: JsonValue
	index: number
	problems: readonly Problem[]
	providers: readonly ProviderOption[]
	previewing: boolean
	actions: QualificationActions
}

function at(problems: readonly Problem[], path: string): Problem[] {
	return problems.filter((problem) => problem.path === path)
}

export function QualificationForm({
	agent,
	index,
	problems,
	providers,
	previewing,
	actions,
}: QualificationFormProps) {
	const base = `qualification_agents[${index}]`
	const key = `qa-${index}`
	const questions = questionsOf(agent)
	const prompt = agentField(agent, 'system_prompt')
	const nameProblems = at(problems, `${base}.name`)
	const idProblems = at(problems, `${base}.id`)
	const providerProblems = at(problems, `${base}.provider_id`)
	const weightProblems = at(problems, `${base}.weight`)
	const promptProblems = at(problems, `${base}.system_prompt`)
	const questionsProblems = at(problems, `${base}.questions`)

	return (
		<div className="flex flex-col gap-5">
			<div className="flex items-end gap-4">
				<div className="flex flex-1 flex-col gap-1.5">
					<Label htmlFor={`${key}-name`}>Nom</Label>
					<Input
						id={`${key}-name`}
						value={agentField(agent, 'name')}
						aria-invalid={nameProblems.length > 0}
						onChange={(event) => actions.setName(index, event.target.value)}
					/>
					<FieldErrors id={`${key}-name`} problems={nameProblems} />
				</div>
				<div className="flex h-9 items-center gap-2">
					<Switch
						id={`${key}-enabled`}
						checked={agentEnabled(agent)}
						onCheckedChange={(enabled) => actions.setEnabled(index, enabled)}
					/>
					<Label htmlFor={`${key}-enabled`}>Activé</Label>
				</div>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label htmlFor={`${key}-provider`}>Fournisseur LLM</Label>
				<ProviderSelect
					id={`${key}-provider`}
					value={agentProvider(agent)}
					providers={providers}
					invalid={providerProblems.length > 0}
					onChange={(providerId) => actions.setProvider(index, providerId)}
				/>
				<FieldErrors id={`${key}-provider`} problems={providerProblems} />
			</div>
			<div className="flex flex-col gap-1.5">
				<Label id={`${key}-weight-label`}>Poids</Label>
				<p className="text-xs text-muted-foreground">
					Poids de cet agent dans la confiance du lead, de 1 à 10.
				</p>
				<WeightPicker
					labelledBy={`${key}-weight-label`}
					value={agentWeight(agent)}
					invalid={weightProblems.length > 0}
					onChange={(weight) => actions.setWeight(index, weight)}
				/>
				<FieldErrors id={`${key}-weight`} problems={weightProblems} />
			</div>
			<div className="flex flex-col gap-1.5">
				<Label htmlFor={`${key}-prompt`}>Prompt système</Label>
				<Textarea
					id={`${key}-prompt`}
					rows={10}
					value={prompt}
					aria-invalid={promptProblems.length > 0}
					onChange={(event) => actions.setPrompt(index, event.target.value)}
				/>
				<p className="text-xs text-muted-foreground">
					{prompt.length} / {MAX_PROMPT_CHARS} caractères
				</p>
				<FieldErrors id={`${key}-prompt`} problems={promptProblems} />
			</div>
			<div className="flex flex-col gap-3">
				<h3 className="text-sm font-semibold">Questions</h3>
				<FieldErrors id={`${key}-questions`} problems={questionsProblems} />
				{Object.keys(questions).map((slot) => {
					const position = Number(slot)
					return (
						<QuestionCard
							key={slot}
							agentIndex={index}
							index={position}
							question={questions[position] ?? null}
							problems={problems}
							removable={questions.length > 1}
							actions={actions}
						/>
					)
				})}
				<div>
					<Button
						type="button"
						variant="outline"
						size="sm"
						onClick={() => actions.addQuestion(index)}
					>
						<PlusIcon />
						Ajouter une question
					</Button>
				</div>
			</div>
			<div className="flex flex-col gap-2">
				<Button
					type="button"
					variant="outline"
					size="sm"
					className="self-start"
					aria-expanded={previewing}
					onClick={actions.togglePreview}
				>
					Aperçu du JSON contractuel
				</Button>
				{previewing ? <ContractPreview schema={contractSchema(agent)} /> : null}
			</div>
			<details
				open={idProblems.length > 0}
				className="rounded-md border px-3 py-2"
			>
				<summary className="cursor-pointer text-sm font-medium">Avancé</summary>
				<div className="mt-3 flex flex-col gap-1.5">
					<Label htmlFor={`${key}-id`}>Identifiant</Label>
					<p className="text-xs text-muted-foreground">
						Identifiant technique, unique parmi tous les agents du profil :
						minuscules, chiffres et tirets. Il suit le nom tant que vous ne le
						modifiez pas.
					</p>
					<Input
						id={`${key}-id`}
						className="font-mono text-xs"
						value={agentField(agent, 'id')}
						aria-invalid={idProblems.length > 0}
						onChange={(event) => actions.setId(index, event.target.value)}
					/>
					<FieldErrors id={`${key}-id`} problems={idProblems} />
				</div>
			</details>
		</div>
	)
}
