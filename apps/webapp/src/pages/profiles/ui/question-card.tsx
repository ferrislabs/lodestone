import { PlusIcon, Trash2Icon } from 'lucide-react'

import { FieldErrors } from '#/components/schema-form/field-frame'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import type { JsonValue, Problem } from '#/lib/schema-form'
import { agentField } from '#/pages/profiles/lib/agents'
import {
	MAX_OPTIONS,
	MIN_OPTIONS,
	numberField,
	optionsOf,
} from '#/pages/profiles/lib/qualification'
import type { QualificationActions } from '#/pages/profiles/lib/qualification-actions'
import { DEFAULT_WEIGHT } from '#/pages/profiles/lib/qualification-templates'
import { WeightPicker } from '#/pages/profiles/ui/weight-picker'

interface QuestionCardProps {
	agentIndex: number
	index: number
	question: JsonValue
	problems: readonly Problem[]
	removable: boolean
	actions: QualificationActions
}

function at(problems: readonly Problem[], path: string): Problem[] {
	return problems.filter((problem) => problem.path === path)
}

export function QuestionCard({
	agentIndex,
	index,
	question,
	problems,
	removable,
	actions,
}: QuestionCardProps) {
	const base = `qualification_agents[${agentIndex}].questions[${index}]`
	const key = `qa-${agentIndex}-q-${index}`
	const options = optionsOf(question)
	const wordingProblems = at(problems, `${base}.wording`)
	const idProblems = at(problems, `${base}.id`)
	const optionsProblems = at(problems, `${base}.options`)
	const weightProblems = at(problems, `${base}.weight`)
	return (
		<section
			aria-label={`Question ${index + 1}`}
			data-question={index}
			className="flex flex-col gap-3 rounded-md border bg-muted/20 p-3"
		>
			<div className="flex items-center justify-between gap-2">
				<p className="text-sm font-semibold">Question {index + 1}</p>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					disabled={!removable}
					aria-label={`Supprimer la question ${index + 1}`}
					onClick={() => actions.removeQuestion(agentIndex, index)}
				>
					<Trash2Icon />
					Supprimer
				</Button>
			</div>
			<div className="flex flex-col gap-1.5">
				<Label htmlFor={`${key}-wording`}>Libellé</Label>
				<Input
					id={`${key}-wording`}
					value={agentField(question, 'wording')}
					aria-invalid={wordingProblems.length > 0}
					onChange={(event) =>
						actions.setQuestionWording(agentIndex, index, event.target.value)
					}
				/>
				<FieldErrors id={`${key}-wording`} problems={wordingProblems} />
			</div>
			<div className="flex flex-col gap-1.5">
				<Label id={`${key}-weight-label`}>Poids de la question</Label>
				<WeightPicker
					labelledBy={`${key}-weight-label`}
					value={numberField(question, 'weight') ?? DEFAULT_WEIGHT}
					invalid={weightProblems.length > 0}
					onChange={(weight) =>
						actions.setQuestionWeight(agentIndex, index, weight)
					}
				/>
				<FieldErrors id={`${key}-weight`} problems={weightProblems} />
			</div>
			<div className="flex flex-col gap-2">
				<p className="text-sm font-medium">Options</p>
				<ul className="flex flex-col gap-2">
					{Object.keys(options).map((slotKey) => {
						const position = Number(slotKey)
						const option = options[position] ?? null
						const optionBase = `${base}.options[${position}]`
						const slot = `${key}-o-${position}`
						const valueProblems = at(problems, `${optionBase}.value`)
						const labelProblems = at(problems, `${optionBase}.label`)
						const scoreProblems = at(problems, `${optionBase}.score`)
						return (
							<li
								key={slotKey}
								data-option={position}
								className="flex flex-col gap-1"
							>
								<div className="grid grid-cols-[1fr_1fr_5rem_auto] items-end gap-2">
									<div className="flex flex-col gap-1">
										<Label htmlFor={`${slot}-value`}>Valeur</Label>
										<Input
											id={`${slot}-value`}
											className="font-mono text-xs"
											value={agentField(option, 'value')}
											aria-invalid={valueProblems.length > 0}
											onChange={(event) =>
												actions.setOptionValue(
													agentIndex,
													index,
													position,
													event.target.value,
												)
											}
										/>
									</div>
									<div className="flex flex-col gap-1">
										<Label htmlFor={`${slot}-label`}>Libellé de l'option</Label>
										<Input
											id={`${slot}-label`}
											value={agentField(option, 'label')}
											aria-invalid={labelProblems.length > 0}
											onChange={(event) =>
												actions.setOptionLabel(
													agentIndex,
													index,
													position,
													event.target.value,
												)
											}
										/>
									</div>
									<div className="flex flex-col gap-1">
										<Label htmlFor={`${slot}-score`}>Score</Label>
										<Input
											id={`${slot}-score`}
											type="number"
											min={0}
											max={1}
											step={0.1}
											inputMode="decimal"
											value={numberField(option, 'score') ?? ''}
											aria-invalid={scoreProblems.length > 0}
											onChange={(event) => {
												const parsed = event.currentTarget.valueAsNumber
												actions.setOptionScore(
													agentIndex,
													index,
													position,
													Number.isNaN(parsed) ? null : parsed,
												)
											}}
										/>
									</div>
									<Button
										type="button"
										variant="ghost"
										size="sm"
										disabled={options.length <= MIN_OPTIONS}
										aria-label={`Supprimer l'option ${position + 1} de la question ${index + 1}`}
										onClick={() =>
											actions.removeOption(agentIndex, index, position)
										}
									>
										<Trash2Icon />
									</Button>
								</div>
								<FieldErrors id={`${slot}-value`} problems={valueProblems} />
								<FieldErrors id={`${slot}-label`} problems={labelProblems} />
								<FieldErrors id={`${slot}-score`} problems={scoreProblems} />
							</li>
						)
					})}
				</ul>
				<FieldErrors id={`${key}-options`} problems={optionsProblems} />
				<div>
					<Button
						type="button"
						variant="outline"
						size="sm"
						disabled={options.length >= MAX_OPTIONS}
						aria-label={`Ajouter une option à la question ${index + 1}`}
						onClick={() => actions.addOption(agentIndex, index)}
					>
						<PlusIcon />
						Ajouter une option
					</Button>
				</div>
			</div>
			<details
				open={idProblems.length > 0}
				className="rounded-md border px-3 py-2"
			>
				<summary className="cursor-pointer text-sm font-medium">Avancé</summary>
				<div className="mt-3 flex flex-col gap-1.5">
					<Label htmlFor={`${key}-id`}>Identifiant de la question</Label>
					<p className="text-xs text-muted-foreground">
						Clé de la réponse dans le JSON contractuel : minuscules, chiffres,
						tirets et tirets bas.
					</p>
					<Input
						id={`${key}-id`}
						className="font-mono text-xs"
						value={agentField(question, 'id')}
						aria-invalid={idProblems.length > 0}
						onChange={(event) =>
							actions.setQuestionId(agentIndex, index, event.target.value)
						}
					/>
					<FieldErrors id={`${key}-id`} problems={idProblems} />
				</div>
			</details>
		</section>
	)
}
