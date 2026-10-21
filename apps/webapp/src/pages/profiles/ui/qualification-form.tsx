import { ChevronDownIcon, ChevronRightIcon, PlusIcon } from 'lucide-react'
import type { ReactNode } from 'react'

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
	MAX_DESCRIPTION_CHARS,
	MAX_PROMPT_CHARS,
	type ProviderOption,
	questionsOf,
} from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'
import { AxisCard } from '#/pages/profiles/ui/axis-card'
import { ContractPreview } from '#/pages/profiles/ui/contract-preview'
import { ProviderSelect } from '#/pages/profiles/ui/provider-select'
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

function FormSection({
	title,
	help,
	children,
}: {
	title: string
	help: string
	children: ReactNode
}) {
	return (
		<section
			aria-label={title}
			className="flex flex-col gap-3 rounded-lg border p-4"
		>
			<div className="flex flex-col gap-0.5">
				<h3 className="text-sm font-semibold">{title}</h3>
				<p className="text-xs text-muted-foreground">{help}</p>
			</div>
			{children}
		</section>
	)
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
	const description = agentField(agent, 'description')
	const nameProblems = at(problems, `${base}.name`)
	const idProblems = at(problems, `${base}.id`)
	const descriptionProblems = at(problems, `${base}.description`)
	const providerProblems = at(problems, `${base}.provider_id`)
	const weightProblems = at(problems, `${base}.weight`)
	const promptProblems = at(problems, `${base}.system_prompt`)
	const questionsProblems = at(problems, `${base}.questions`)
	const PreviewIcon = previewing ? ChevronDownIcon : ChevronRightIcon

	return (
		<div className="flex flex-col gap-4">
			<FormSection
				title="Agent"
				help="Qui est cet agent, ce qu'il évalue et quel modèle lui répond."
			>
				<div className="flex items-start gap-4">
					<div className="flex flex-1 flex-col gap-1.5">
						<Label htmlFor={`${key}-name`}>Nom</Label>
						<Input
							id={`${key}-name`}
							value={agentField(agent, 'name')}
							aria-invalid={nameProblems.length > 0}
							onChange={(event) => actions.setName(index, event.target.value)}
						/>
						<FieldErrors id={`${key}-name`} problems={nameProblems} />
						<div className="flex items-center gap-2">
							<Label
								htmlFor={`${key}-id`}
								className="shrink-0 text-xs text-muted-foreground"
							>
								Identifiant
							</Label>
							<Input
								id={`${key}-id`}
								className="h-7 font-mono text-xs"
								value={agentField(agent, 'id')}
								aria-invalid={idProblems.length > 0}
								onChange={(event) => actions.setId(index, event.target.value)}
							/>
						</div>
						<FieldErrors id={`${key}-id`} problems={idProblems} />
						<p className="text-xs text-muted-foreground">
							Unique parmi tous les agents du profil : minuscules, chiffres et
							tirets. Il suit le nom tant que vous ne le modifiez pas.
						</p>
					</div>
					<div className="flex h-9 items-center gap-2 pt-6">
						<Switch
							id={`${key}-enabled`}
							checked={agentEnabled(agent)}
							onCheckedChange={(enabled) => actions.setEnabled(index, enabled)}
						/>
						<Label htmlFor={`${key}-enabled`}>Activé</Label>
					</div>
				</div>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor={`${key}-description`}>Ce que cet agent évalue</Label>
					<Textarea
						id={`${key}-description`}
						rows={2}
						value={description}
						aria-invalid={descriptionProblems.length > 0}
						onChange={(event) =>
							actions.setDescription(index, event.target.value)
						}
					/>
					<p className="text-xs text-muted-foreground">
						Affiché sur la carte de l'agent et dans la fiche du lead.{' '}
						{description.length} / {MAX_DESCRIPTION_CHARS} caractères
					</p>
					<FieldErrors
						id={`${key}-description`}
						problems={descriptionProblems}
					/>
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
					<Label id={`${key}-weight-label`}>Poids de l'agent</Label>
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
			</FormSection>
			<FormSection
				title="Prompt système"
				help="La consigne donnée au modèle. Le contrat JSON et le contexte du marché sont ajoutés automatiquement."
			>
				<div className="flex flex-col gap-1.5">
					<Label htmlFor={`${key}-prompt`} className="sr-only">
						Prompt système
					</Label>
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
			</FormSection>
			<FormSection
				title="Axes d'analyse"
				help="Chaque axe est une question à choix que l'agent mesure sur le lead. Ses options valent un score de 0 à 1."
			>
				<FieldErrors id={`${key}-questions`} problems={questionsProblems} />
				{Object.keys(questions).map((slot) => {
					const position = Number(slot)
					return (
						<AxisCard
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
						Ajouter un axe d'analyse
					</Button>
				</div>
			</FormSection>
			<section
				aria-label="Aperçu du JSON contractuel"
				className="flex flex-col gap-2 rounded-lg border p-4"
			>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					className="justify-start self-start px-1 text-sm font-semibold"
					aria-expanded={previewing}
					onClick={actions.togglePreview}
				>
					<PreviewIcon />
					Aperçu du JSON contractuel
				</Button>
				<p className="text-xs text-muted-foreground">
					Ce que le modèle doit renvoyer : un résumé et une réponse par axe.
				</p>
				{previewing ? <ContractPreview schema={contractSchema(agent)} /> : null}
			</section>
		</div>
	)
}
