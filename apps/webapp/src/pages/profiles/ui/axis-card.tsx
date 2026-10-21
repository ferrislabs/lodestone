import { PlusIcon, Trash2Icon } from 'lucide-react'

import { FieldErrors } from '#/components/schema-form/field-frame'
import { Button } from '#/components/ui/button'
import { Input } from '#/components/ui/input'
import { Label } from '#/components/ui/label'
import { Switch } from '#/components/ui/switch'
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
	displayOrder,
	isExcludingOption,
	isUnknownOption,
	MAX_OPTIONS,
	MIN_OPTIONS,
	numberField,
	optionsOf,
} from '#/pages/profiles/lib/qualification'
import type { AxisActions } from '#/pages/profiles/lib/qualification-actions'
import { DEFAULT_WEIGHT } from '#/pages/profiles/lib/qualification-templates'
import { WeightPicker } from '#/pages/profiles/ui/weight-picker'

export type AxisKind = 'qualification' | 'scoring'

interface AxisWords {
	title: string
	of: string
	the: string
	to: string
	listPath: string
	itemsPath: string
	keyPrefix: string
}

const AXIS_WORDS: Record<AxisKind, AxisWords> = {
	qualification: {
		title: 'Axe',
		of: "de l'axe",
		the: "l'axe",
		to: "à l'axe",
		listPath: 'qualification_agents',
		itemsPath: 'questions',
		keyPrefix: 'qa',
	},
	scoring: {
		title: 'Critère',
		of: 'du critère',
		the: 'le critère',
		to: 'au critère',
		listPath: 'scoring_agents',
		itemsPath: 'criteria',
		keyPrefix: 'sa',
	},
}

interface AxisCardProps {
	agentIndex: number
	index: number
	question: JsonValue
	problems: readonly Problem[]
	removable: boolean
	actions: AxisActions
	kind?: AxisKind
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
	kind = 'qualification',
}: AxisCardProps) {
	const words = AXIS_WORDS[kind]
	const excludable = kind === 'qualification'
	const base = `${words.listPath}[${agentIndex}].${words.itemsPath}[${index}]`
	const key = `${words.keyPrefix}-${agentIndex}-q-${index}`
	const options = optionsOf(question)
	const wordingProblems = at(problems, `${base}.wording`)
	const idProblems = at(problems, `${base}.id`)
	const optionsProblems = at(problems, `${base}.options`)
	const weightProblems = at(problems, `${base}.weight`)
	return (
		<section
			aria-label={`${words.title} ${index + 1}`}
			data-question={index}
			className="flex flex-col gap-3 rounded-md border bg-muted/20 p-3"
		>
			<div className="flex items-center justify-between gap-2">
				<p className="text-sm font-semibold">
					{words.title} {index + 1}
				</p>
				<Button
					type="button"
					variant="ghost"
					size="sm"
					disabled={!removable}
					aria-label={`Supprimer ${words.the} ${index + 1}`}
					onClick={() => actions.removeQuestion(agentIndex, index)}
				>
					<Trash2Icon />
					Supprimer
				</Button>
			</div>
			<div className="grid gap-3 sm:grid-cols-[1fr_13rem]">
				<div className="flex flex-col gap-1.5">
					<Label htmlFor={`${key}-wording`}>Intitulé {words.of}</Label>
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
						Identifiant {words.of}
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
				<Label id={`${key}-weight-label`}>Poids {words.of}</Label>
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
				<p className="text-xs text-muted-foreground">
					Le modèle cite une phrase des documents pour justifier son choix. Sans
					citation vérifiée, ou quand rien ne permet de répondre, la réponse
					devient l'option « inconnu », dont le score reste modifiable.
					{excludable
						? " Une option qui exclut le lead écarte le lead quand l'agent la choisit avec une probabilité d'au moins le seuil d'exclusion de l'agent."
						: ' Un critère ne peut pas exclure le lead.'}
				</p>
				<Table data-testid={`${key}-options`}>
					<TableHeader>
						<TableRow>
							<TableHead className="h-8 px-1 text-xs">Valeur</TableHead>
							<TableHead className="h-8 px-1 text-xs">Libellé</TableHead>
							<TableHead className="h-8 w-24 px-1 text-xs">Score</TableHead>
							{excludable ? (
								<TableHead className="h-8 w-28 px-1 text-xs">
									Exclusion
								</TableHead>
							) : null}
							<TableHead className="h-8 w-10 px-1">
								<span className="sr-only">Actions</span>
							</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{displayOrder(options).map((position) => {
							const slotKey = String(position)
							const option = options[position] ?? null
							const unknown = isUnknownOption(option)
							const optionBase = `${base}.options[${position}]`
							const slot = `${key}-o-${position}`
							const valueProblems = at(problems, `${optionBase}.value`)
							const labelProblems = at(problems, `${optionBase}.label`)
							const scoreProblems = at(problems, `${optionBase}.score`)
							return (
								<TableRow
									key={slotKey}
									data-option={position}
									data-unknown={unknown ? 'true' : undefined}
									className="border-0 align-top"
								>
									<TableCell className="px-1 py-1">
										{unknown ? (
											<p
												className="flex h-9 items-center text-xs font-medium"
												data-part="unknown-option"
											>
												Option « inconnu »
												<code className="ml-1.5 font-mono text-[11px] text-muted-foreground">
													{agentField(option, 'value')}
												</code>
											</p>
										) : (
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
										)}
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
									{excludable ? (
										<TableCell className="px-1 py-1">
											<div className="flex h-9 items-center gap-2">
												<Switch
													id={`${slot}-excludes`}
													aria-label={`Exclut le lead : option ${position + 1}`}
													checked={isExcludingOption(option)}
													disabled={unknown}
													title={
														unknown
															? "L'option « inconnu » ne peut pas exclure le lead"
															: undefined
													}
													onCheckedChange={(excludes) =>
														actions.setOptionExcludes?.(
															agentIndex,
															index,
															position,
															excludes,
														)
													}
												/>
												<Label
													htmlFor={`${slot}-excludes`}
													className="text-xs font-normal"
												>
													Exclut le lead
												</Label>
											</div>
											<FieldErrors
												id={`${slot}-excludes`}
												problems={at(problems, `${optionBase}.excludes`)}
											/>
										</TableCell>
									) : null}
									<TableCell className="px-1 py-1">
										<Button
											type="button"
											variant="ghost"
											size="icon"
											disabled={unknown || options.length <= MIN_OPTIONS}
											aria-label={`Supprimer l'option ${position + 1} ${words.of} ${index + 1}`}
											title={
												unknown
													? "L'option « inconnu » est obligatoire"
													: undefined
											}
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
						aria-label={`Ajouter une option ${words.to} ${index + 1}`}
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
