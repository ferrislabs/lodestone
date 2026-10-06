import { PlusIcon, Trash2Icon } from 'lucide-react'

import { FieldErrors } from '#/components/schema-form/field-frame'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
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

interface AxisCardProps {
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

export function AxisCard({
	agentIndex,
	index,
	question,
	problems,
	removable,
	actions,
}: AxisCardProps) {
	const base = `qualification_agents[${agentIndex}].questions[${index}]`
	const key = `qa-${agentIndex}-q-${index}`
	const options = optionsOf(question)
	const wordingProblems = at(problems, `${base}.wording`)
	const idProblems = at(problems, `${base}.id`)
	const optionsProblems = at(problems, `${base}.options`)
	const weightProblems = at(problems, `${base}.weight`)
	return (
		<section
			aria-label={`Axe ${index + 1}`}
			data-question={index}
			className="flex flex-col gap-3 rounded-md border bg-muted/20 p-3"
		>
			<div className="flex items-center justify-between gap-2">
				<p className="text-sm font-semibold">Axe {index + 1}</p>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					disabled={!removable}
					aria-label={`Supprimer l'axe ${index + 1}`}
					onClick={() => actions.removeQuestion(agentIndex, index)}
				>
					<Trash2Icon />
					Supprimer
				</Button>
			</div>
			<div className="grid gap-3 sm:grid-cols-[1fr_13rem]">
				<div className="flex flex-col gap-1.5">
					<Label htmlFor={`${key}-wording`}>Intitulé de l'axe</Label>
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
					<Label htmlFor={`${key}-id`} className="text-xs">
						Identifiant de l'axe
					</Label>
					<Input
						id={`${key}-id`}
						className="h-9 font-mono text-xs"
						value={agentField(question, 'id')}
						aria-invalid={idProblems.length > 0}
						onChange={(event) =>
							actions.setQuestionId(agentIndex, index, event.target.value)
						}
					/>
					<FieldErrors id={`${key}-id`} problems={idProblems} />
				</div>
			</div>
			<p className="text-xs text-muted-foreground">
				L'identifiant est la clé de la réponse dans le JSON contractuel :
				minuscules, chiffres, tirets et tirets bas.
			</p>
			<div className="flex flex-col gap-1.5">
				<Label id={`${key}-weight-label`}>Poids de l'axe</Label>
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
				<Table data-testid={`${key}-options`}>
					<TableHeader>
						<TableRow>
							<TableHead className="h-8 px-1 text-xs">Valeur</TableHead>
							<TableHead className="h-8 px-1 text-xs">Libellé</TableHead>
							<TableHead className="h-8 w-24 px-1 text-xs">Score</TableHead>
							<TableHead className="h-8 w-10 px-1">
								<span className="sr-only">Actions</span>
							</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{Object.keys(options).map((slotKey) => {
							const position = Number(slotKey)
							const option = options[position] ?? null
							const optionBase = `${base}.options[${position}]`
							const slot = `${key}-o-${position}`
							const valueProblems = at(problems, `${optionBase}.value`)
							const labelProblems = at(problems, `${optionBase}.label`)
							const scoreProblems = at(problems, `${optionBase}.score`)
							return (
								<TableRow
									key={slotKey}
									data-option={position}
									className="border-0 align-top"
								>
									<TableCell className="px-1 py-1">
										<Input
											id={`${slot}-value`}
											className="font-mono text-xs"
											aria-label={`Valeur de l'option ${position + 1}`}
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
										<FieldErrors
											id={`${slot}-value`}
											problems={valueProblems}
										/>
									</TableCell>
									<TableCell className="px-1 py-1">
										<Input
											id={`${slot}-label`}
											aria-label={`Libellé de l'option ${position + 1}`}
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
										<FieldErrors
											id={`${slot}-label`}
											problems={labelProblems}
										/>
									</TableCell>
									<TableCell className="px-1 py-1">
										<Input
											id={`${slot}-score`}
											type="number"
											min={0}
											max={1}
											step={0.1}
											inputMode="decimal"
											aria-label={`Score de l'option ${position + 1}`}
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
										<FieldErrors
											id={`${slot}-score`}
											problems={scoreProblems}
										/>
									</TableCell>
									<TableCell className="px-1 py-1">
										<Button
											type="button"
											variant="ghost"
											size="icon"
											disabled={options.length <= MIN_OPTIONS}
											aria-label={`Supprimer l'option ${position + 1} de l'axe ${index + 1}`}
											onClick={() =>
												actions.removeOption(agentIndex, index, position)
											}
										>
											<Trash2Icon />
										</Button>
									</TableCell>
								</TableRow>
							)
						})}
					</TableBody>
				</Table>
				<FieldErrors id={`${key}-options`} problems={optionsProblems} />
				<div>
					<Button
						type="button"
						variant="outline"
						size="sm"
						disabled={options.length >= MAX_OPTIONS}
						aria-label={`Ajouter une option à l'axe ${index + 1}`}
						onClick={() => actions.addOption(agentIndex, index)}
					>
						<PlusIcon />
						Ajouter une option
					</Button>
				</div>
			</div>
		</section>
	)
}
