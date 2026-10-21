import { pluralize } from '#/lib/labels'
import {
	appendIn,
	isJsonObject,
	type JsonObject,
	type JsonValue,
	type Problem,
	removeIn,
	setIn,
} from '#/lib/schema-form'
import {
	agentField,
	agentList,
	slugify,
	uniqueId,
} from '#/pages/profiles/lib/agents'
import {
	DEFAULT_WEIGHT,
	findTemplate,
	templateAgent,
} from '#/pages/profiles/lib/qualification-templates'

export {
	DEFAULT_AGGREGATOR_PROMPT,
	QUALIFICATION_TEMPLATES,
} from '#/pages/profiles/lib/qualification-templates'

export const MIN_OPTIONS = 2
export const MAX_OPTIONS = 10
export const MAX_PROMPT_CHARS = 8000
export const MAX_DESCRIPTION_CHARS = 300
export const MAX_SUMMARY_CHARS = 600
export const DEFAULT_RELIABLE_FROM = 70
export const DEFAULT_DISCARD_BELOW = 30

const IDENTIFIER = /^[a-z0-9][a-z0-9_-]{0,62}$/
const SLUG = /^[a-z0-9][a-z0-9-]{0,62}$/
const GENERATED_QUESTION = /^question-\d+$/

export interface ProviderOption {
	id: string
	name: string
	enabled: boolean
}

export function qualificationList(value: JsonObject): JsonValue[] {
	const agents = value.qualification_agents
	return Array.isArray(agents) ? agents : []
}

export function withQualification(
	value: JsonObject,
	agents: readonly JsonValue[],
): JsonObject {
	return { ...value, qualification_agents: [...agents] }
}

export function allAgentIds(
	value: JsonObject,
	except: number | null = null,
): Set<string> {
	const ids = new Set<string>()
	for (const agent of agentList(value)) ids.add(agentField(agent, 'id'))
	qualificationList(value).forEach((agent, index) => {
		if (index !== except) ids.add(agentField(agent, 'id'))
	})
	return ids
}

export function agentProvider(agent: JsonValue | undefined): string {
	if (!isJsonObject(agent)) return ''
	return typeof agent.provider_id === 'string' ? agent.provider_id : ''
}

export function numberField(
	node: JsonValue | undefined,
	key: string,
): number | null {
	if (!isJsonObject(node)) return null
	const field = node[key]
	return typeof field === 'number' && !Number.isNaN(field) ? field : null
}

export function agentWeight(agent: JsonValue | undefined): number {
	return numberField(agent, 'weight') ?? DEFAULT_WEIGHT
}

export function listField(
	node: JsonValue | undefined,
	key: string,
): JsonValue[] {
	if (!isJsonObject(node)) return []
	const items = node[key]
	return Array.isArray(items) ? items : []
}

export function questionsOf(agent: JsonValue | undefined): JsonValue[] {
	return listField(agent, 'questions')
}

export function optionsOf(question: JsonValue | undefined): JsonValue[] {
	return listField(question, 'options')
}

export function aggregatorOf(value: JsonObject): JsonObject {
	return isJsonObject(value.aggregator) ? value.aggregator : {}
}

export interface Verdict {
	reliableFrom: number | null
	discardBelow: number | null
}

export function verdictOf(value: JsonObject): Verdict {
	const verdict = value.verdict
	if (!isJsonObject(verdict)) {
		return {
			reliableFrom: DEFAULT_RELIABLE_FROM,
			discardBelow: DEFAULT_DISCARD_BELOW,
		}
	}
	const read = (key: string, fallback: number) =>
		key in verdict ? numberField(verdict, key) : fallback
	return {
		reliableFrom: read('reliable_from', DEFAULT_RELIABLE_FROM),
		discardBelow: read('discard_below', DEFAULT_DISCARD_BELOW),
	}
}

export function isPercent(value: number | null): value is number {
	return value !== null && Number.isInteger(value) && value >= 0 && value <= 100
}

export const VERDICT_ORDER_ERROR =
	'« Fiable à partir de » doit être supérieur à « Écarté sous »'
export const PERCENT_ERROR = 'Doit être un pourcentage entier de 0 à 100'

export function verdictProblems(verdict: Verdict): Problem[] {
	const problems: Problem[] = []
	if (!isPercent(verdict.reliableFrom)) {
		problems.push({ path: 'verdict.reliable_from', message: PERCENT_ERROR })
	}
	if (!isPercent(verdict.discardBelow)) {
		problems.push({ path: 'verdict.discard_below', message: PERCENT_ERROR })
	}
	if (
		problems.length === 0 &&
		isPercent(verdict.reliableFrom) &&
		isPercent(verdict.discardBelow) &&
		verdict.reliableFrom <= verdict.discardBelow
	) {
		problems.push({
			path: 'verdict.reliable_from',
			message: VERDICT_ORDER_ERROR,
		})
	}
	return problems
}

export interface VerdictBands {
	discarded: number
	toCheck: number
	reliable: number
}

export function verdictBands(verdict: Verdict): VerdictBands | null {
	if (verdictProblems(verdict).length > 0) return null
	const reliableFrom = verdict.reliableFrom ?? 0
	const discardBelow = verdict.discardBelow ?? 0
	return {
		discarded: discardBelow,
		toCheck: reliableFrom - discardBelow,
		reliable: 100 - reliableFrom,
	}
}

function option(value: string, label: string, score: number): JsonObject {
	return { value, label, score }
}

export function newQualificationAgent(
	templateId: string,
	taken: ReadonlySet<string>,
): JsonObject {
	const template = findTemplate(templateId)
	return templateAgent(
		template,
		uniqueId(slugify(template.name) || 'agent-qualification', taken),
	)
}

export function renameQualificationAgent(
	value: JsonObject,
	index: number,
	name: string,
): JsonObject {
	const agents = qualificationList(value)
	const agent = agents[index]
	if (!isJsonObject(agent)) return value
	const id = agentField(agent, 'id')
	const base = slugify(agentField(agent, 'name')) || 'agent-qualification'
	const untouched = id === base || id.replace(/-\d+$/, '') === base
	const next: JsonObject = { ...agent, name }
	if (untouched) {
		next.id = uniqueId(
			slugify(name) || 'agent-qualification',
			allAgentIds(value, index),
		)
	}
	return withQualification(
		value,
		agents.map((item, position) => (position === index ? next : item)),
	)
}

export function setAgentField(
	value: JsonObject,
	index: number,
	key: string,
	next: JsonValue,
): JsonObject {
	return toObject(
		setIn(value, ['qualification_agents', index, key], next),
		value,
	)
}

function toObject(next: JsonValue, fallback: JsonObject): JsonObject {
	return isJsonObject(next) ? next : fallback
}

function nextQuestionId(questions: readonly JsonValue[]): string {
	const taken = new Set(questions.map((question) => agentField(question, 'id')))
	let counter = questions.length + 1
	while (taken.has(`question-${counter}`)) counter += 1
	return `question-${counter}`
}

export function addQuestion(value: JsonObject, agent: number): JsonObject {
	const questions = questionsOf(qualificationList(value)[agent])
	const created: JsonObject = {
		id: nextQuestionId(questions),
		wording: '',
		weight: DEFAULT_WEIGHT,
		options: [
			option('option-1', 'Option 1', 1),
			option('option-2', 'Option 2', 0),
		],
	}
	return toObject(
		appendIn(value, ['qualification_agents', agent, 'questions'], created),
		value,
	)
}

export function removeQuestion(
	value: JsonObject,
	agent: number,
	question: number,
): JsonObject {
	return toObject(
		removeIn(value, ['qualification_agents', agent, 'questions', question]),
		value,
	)
}

export function setQuestionField(
	value: JsonObject,
	agent: number,
	question: number,
	key: string,
	next: JsonValue,
): JsonObject {
	return toObject(
		setIn(
			value,
			['qualification_agents', agent, 'questions', question, key],
			next,
		),
		value,
	)
}

export function setQuestionWording(
	value: JsonObject,
	agent: number,
	question: number,
	wording: string,
): JsonObject {
	const questions = questionsOf(qualificationList(value)[agent])
	const current = questions[question]
	const id = agentField(current, 'id')
	const previous = slugify(agentField(current, 'wording'))
	const untouched =
		GENERATED_QUESTION.test(id) || (previous !== '' && id === previous)
	const withWording = setQuestionField(
		value,
		agent,
		question,
		'wording',
		wording,
	)
	const derived = slugify(wording)
	if (!untouched || derived === '') return withWording
	const taken = new Set(
		questions
			.filter((_, position) => position !== question)
			.map((item) => agentField(item, 'id')),
	)
	return setQuestionField(
		withWording,
		agent,
		question,
		'id',
		uniqueId(derived, taken),
	)
}

export function addOption(
	value: JsonObject,
	agent: number,
	question: number,
): JsonObject {
	const options = optionsOf(
		questionsOf(qualificationList(value)[agent])[question],
	)
	const taken = new Set(options.map((item) => agentField(item, 'value')))
	let counter = options.length + 1
	while (taken.has(`option-${counter}`)) counter += 1
	return toObject(
		appendIn(
			value,
			['qualification_agents', agent, 'questions', question, 'options'],
			option(`option-${counter}`, `Option ${counter}`, 0),
		),
		value,
	)
}

export function removeOption(
	value: JsonObject,
	agent: number,
	question: number,
	position: number,
): JsonObject {
	return toObject(
		removeIn(value, [
			'qualification_agents',
			agent,
			'questions',
			question,
			'options',
			position,
		]),
		value,
	)
}

export function setOptionField(
	value: JsonObject,
	agent: number,
	question: number,
	position: number,
	key: string,
	next: JsonValue,
): JsonObject {
	return toObject(
		setIn(
			value,
			[
				'qualification_agents',
				agent,
				'questions',
				question,
				'options',
				position,
				key,
			],
			next,
		),
		value,
	)
}

export function setAggregatorField(
	value: JsonObject,
	key: string,
	next: JsonValue,
): JsonObject {
	return toObject(setIn(value, ['aggregator', key], next), value)
}

export function setVerdictField(
	value: JsonObject,
	key: 'reliable_from' | 'discard_below',
	next: number | null,
): JsonObject {
	const current = isJsonObject(value.verdict) ? value.verdict : {}
	return { ...value, verdict: { ...current, [key]: next } }
}

export function providerName(
	id: string,
	providers: readonly ProviderOption[],
): string {
	if (id === '') return 'par défaut'
	return providers.find((provider) => provider.id === id)?.name ?? 'introuvable'
}

export function qualificationSummary(
	agent: JsonValue | undefined,
	providers: readonly ProviderOption[],
): string {
	const count = questionsOf(agent).length
	return [
		`${count} ${pluralize(count, "axe d'analyse", "axes d'analyse")}`,
		`fournisseur : ${providerName(agentProvider(agent), providers)}`,
		`poids ${agentWeight(agent)}`,
	].join(' · ')
}

function probabilitySchema(): JsonObject {
	return { type: 'number', minimum: 0, maximum: 1 }
}

function questionSchema(question: JsonValue): JsonObject {
	const values = optionsOf(question).map((item) => agentField(item, 'value'))
	return {
		description: agentField(question, 'wording'),
		type: 'object',
		properties: {
			type: { type: 'string', const: 'choice' },
			choice: { type: 'string', enum: values },
			confidence: probabilitySchema(),
			probabilities: {
				type: 'object',
				properties: Object.fromEntries(
					values.map((item) => [item, probabilitySchema()]),
				),
				required: values,
				additionalProperties: false,
			},
		},
		required: ['type', 'choice', 'confidence', 'probabilities'],
		additionalProperties: false,
	}
}

export function contractSchema(agent: JsonValue | undefined): JsonObject {
	const questions = questionsOf(agent)
	const ids = questions.map((question) => agentField(question, 'id'))
	return {
		$schema: 'https://json-schema.org/draft/2020-12/schema',
		title: `Answers of ${agentField(agent, 'name')}`,
		type: 'object',
		properties: {
			summary: { type: 'string', minLength: 1, maxLength: MAX_SUMMARY_CHARS },
			answers: {
				type: 'object',
				properties: Object.fromEntries(
					questions.map((question, position) => [
						ids[position] ?? '',
						questionSchema(question),
					]),
				),
				required: ids,
				additionalProperties: false,
			},
		},
		required: ['summary', 'answers'],
		additionalProperties: false,
	}
}

export const CONTRACT_EXAMPLE = `{
  "summary": "<2 à 4 phrases sur ce que l'agent a observé>",
  "answers": {
    "<identifiant de l'axe>": {
      "type": "choice",
      "choice": "<valeur de l'option>",
      "confidence": 0.0,
      "probabilities": { "<valeur de l'option>": 0.0 }
    }
  }
}`

function problem(path: string, message: string): Problem {
	return { path, message }
}

function checkOptions(
	options: readonly JsonValue[],
	base: string,
	problems: Problem[],
) {
	const path = `${base}.options`
	if (options.length < MIN_OPTIONS || options.length > MAX_OPTIONS) {
		problems.push(problem(path, 'Doit avoir de 2 à 10 options'))
	}
	const seen = new Set<string>()
	options.forEach((item, position) => {
		const itemPath = `${path}[${position}]`
		const optionValue = agentField(item, 'value')
		if (optionValue !== '' && !IDENTIFIER.test(optionValue)) {
			problems.push(
				problem(
					`${itemPath}.value`,
					'Doit être composé de minuscules, chiffres, tirets et tirets bas',
				),
			)
		} else if (optionValue !== '' && seen.has(optionValue)) {
			problems.push(problem(`${itemPath}.value`, "Doit être unique dans l'axe"))
		}
		seen.add(optionValue)
		const score = numberField(item, 'score')
		if (score !== null && (score < 0 || score > 1)) {
			problems.push(
				problem(`${itemPath}.score`, 'Doit être un nombre de 0 à 1'),
			)
		}
	})
}

function checkQuestions(
	questions: readonly JsonValue[],
	base: string,
	problems: Problem[],
) {
	const path = `${base}.questions`
	if (questions.length === 0) {
		problems.push(problem(path, "Doit contenir au moins un axe d'analyse"))
		return
	}
	const seen = new Set<string>()
	questions.forEach((question, index) => {
		const questionPath = `${path}[${index}]`
		const id = agentField(question, 'id')
		if (id !== '' && !IDENTIFIER.test(id)) {
			problems.push(
				problem(
					`${questionPath}.id`,
					'Doit être composé de minuscules, chiffres, tirets et tirets bas',
				),
			)
		} else if (id !== '' && seen.has(id)) {
			problems.push(
				problem(`${questionPath}.id`, "Doit être unique dans l'agent"),
			)
		}
		seen.add(id)
		checkOptions(optionsOf(question), questionPath, problems)
	})
}

export function validateQualification(value: JsonObject): Problem[] {
	const agents = qualificationList(value)
	const problems: Problem[] = []
	if (agents.length === 0) {
		problems.push(
			problem(
				'qualification_agents',
				'Doit contenir au moins un agent de qualification',
			),
		)
	}
	const seen = new Set<string>(
		agentList(value).map((agent) => agentField(agent, 'id')),
	)
	agents.forEach((agent, index) => {
		const base = `qualification_agents[${index}]`
		const id = agentField(agent, 'id')
		if (id !== '' && !SLUG.test(id)) {
			problems.push(
				problem(
					`${base}.id`,
					'Doit être composé de minuscules, chiffres et tirets',
				),
			)
		} else if (id !== '' && seen.has(id)) {
			problems.push(
				problem(
					`${base}.id`,
					'Doit être unique parmi tous les agents du profil',
				),
			)
		}
		seen.add(id)
		const description = agentField(agent, 'description')
		if (description.length > MAX_DESCRIPTION_CHARS) {
			problems.push(
				problem(
					`${base}.description`,
					`Doit contenir au maximum ${MAX_DESCRIPTION_CHARS} caractères`,
				),
			)
		}
		if (agentField(agent, 'system_prompt').length > MAX_PROMPT_CHARS) {
			problems.push(
				problem(
					`${base}.system_prompt`,
					`Doit contenir au maximum ${MAX_PROMPT_CHARS} caractères`,
				),
			)
		}
		checkQuestions(questionsOf(agent), base, problems)
	})
	const aggregator = aggregatorOf(value)
	if (agentField(aggregator, 'system_prompt').length > MAX_PROMPT_CHARS) {
		problems.push(
			problem(
				'aggregator.system_prompt',
				`Doit contenir au maximum ${MAX_PROMPT_CHARS} caractères`,
			),
		)
	}
	problems.push(...verdictProblems(verdictOf(value)))
	return problems
}
