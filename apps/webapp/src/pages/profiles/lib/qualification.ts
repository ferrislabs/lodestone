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
	DEFAULT_EXCLUSION_THRESHOLD,
	DEFAULT_WEIGHT,
	findTemplate,
	templateAgent,
} from '#/pages/profiles/lib/qualification-templates'

export {
	DEFAULT_AGGREGATOR_PROMPT,
	QUALIFICATION_TEMPLATES,
} from '#/pages/profiles/lib/qualification-templates'

export const MIN_OPTIONS = 2
export const MAX_OPTIONS = 11
export const MIN_EXCLUSION_PERCENT = 50
export const MIN_SAMPLES = 1
export const MAX_SAMPLES = 5
export const DEFAULT_SAMPLES = 3
export const MAX_SAMPLING_TEMPERATURE = 1.5
export const DEFAULT_SAMPLING_TEMPERATURE = 0.7
export const DEFAULT_BAND_LOW = 40
export const DEFAULT_BAND_HIGH = 70
export const MAX_EVIDENCE_CHARS = 300
export const UNKNOWN_VALUE = 'inconnu'
export const UNKNOWN_LABEL = 'Inconnu'
export const UNKNOWN_SCORE = 0.5
export const MAX_PROMPT_CHARS = 8000
export const MAX_DESCRIPTION_CHARS = 300
export const MAX_SUMMARY_CHARS = 600
export const DEFAULT_RELIABLE_FROM = 70
export const DEFAULT_DISCARD_BELOW = 30
export const DEFAULT_CALIBRATION_FLOOR = 0.05
export const MAX_CALIBRATION_FLOOR_PERCENT = 20
export const DEFAULT_MIN_EVIDENCE = 0.5
export const DEFAULT_INFLUENCE = 0.6
export const DEFAULT_REFERENCE_SCORE = 50
export const DEFAULT_EVIDENCE_WEIGHT = 1
export const MAX_EVIDENCE_WEIGHT = 10

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

export function isUnknownOption(option: JsonValue | undefined): boolean {
	return isJsonObject(option) && option.is_unknown === true
}

export function isExcludingOption(option: JsonValue | undefined): boolean {
	return isJsonObject(option) && option.excludes === true
}

export function exclusionThresholdOf(
	agent: JsonValue | undefined,
): number | null {
	if (!isJsonObject(agent)) return null
	if (!('exclusion_threshold' in agent)) {
		return Math.round(DEFAULT_EXCLUSION_THRESHOLD * 100)
	}
	const fraction = numberField(agent, 'exclusion_threshold')
	return fraction === null ? null : Math.round(fraction * 100)
}

export function samplesOf(agent: JsonValue | undefined): number | null {
	if (!isJsonObject(agent)) return null
	if (!('samples' in agent)) return DEFAULT_SAMPLES
	return numberField(agent, 'samples')
}

export function samplingTemperatureOf(
	agent: JsonValue | undefined,
): number | null {
	if (!isJsonObject(agent)) return null
	if (!('sampling_temperature' in agent)) return DEFAULT_SAMPLING_TEMPERATURE
	return numberField(agent, 'sampling_temperature')
}

export function displayOrder(options: readonly JsonValue[]): number[] {
	const positions = options.map((_, position) => position)
	return [
		...positions.filter((position) => !isUnknownOption(options[position])),
		...positions.filter((position) => isUnknownOption(options[position])),
	]
}

export function aggregatorOf(value: JsonObject): JsonObject {
	return isJsonObject(value.aggregator) ? value.aggregator : {}
}

export interface Escalation {
	provider: string
	bandLow: number | null
	bandHigh: number | null
}

export function escalationOf(value: JsonObject): Escalation {
	const escalation = value.escalation
	if (!isJsonObject(escalation)) {
		return {
			provider: '',
			bandLow: DEFAULT_BAND_LOW,
			bandHigh: DEFAULT_BAND_HIGH,
		}
	}
	const read = (key: string, fallback: number) =>
		key in escalation ? numberField(escalation, key) : fallback
	return {
		provider:
			typeof escalation.provider === 'string' ? escalation.provider : '',
		bandLow: read('band_low', DEFAULT_BAND_LOW),
		bandHigh: read('band_high', DEFAULT_BAND_HIGH),
	}
}

export const BAND_ORDER_ERROR =
	"« Jusqu'à » doit être supérieur à « À partir de »"
export const SAMPLES_ERROR = `Doit être un entier de ${MIN_SAMPLES} à ${MAX_SAMPLES}`
export const SAMPLING_TEMPERATURE_ERROR = `Doit être un nombre de 0 à ${MAX_SAMPLING_TEMPERATURE}`

export function escalationProblems(escalation: Escalation): Problem[] {
	const problems: Problem[] = []
	if (!isPercent(escalation.bandLow)) {
		problems.push({ path: 'escalation.band_low', message: PERCENT_ERROR })
	}
	if (!isPercent(escalation.bandHigh)) {
		problems.push({ path: 'escalation.band_high', message: PERCENT_ERROR })
	}
	if (
		problems.length === 0 &&
		isPercent(escalation.bandLow) &&
		isPercent(escalation.bandHigh) &&
		escalation.bandHigh <= escalation.bandLow
	) {
		problems.push({ path: 'escalation.band_high', message: BAND_ORDER_ERROR })
	}
	return problems
}

export function samplingProblems(
	agent: JsonValue | undefined,
	base: string,
): Problem[] {
	const problems: Problem[] = []
	const samples = samplesOf(agent)
	if (
		samples === null ||
		!Number.isInteger(samples) ||
		samples < MIN_SAMPLES ||
		samples > MAX_SAMPLES
	) {
		problems.push({ path: `${base}.samples`, message: SAMPLES_ERROR })
	}
	const temperature = samplingTemperatureOf(agent)
	if (
		temperature === null ||
		temperature < 0 ||
		temperature > MAX_SAMPLING_TEMPERATURE
	) {
		problems.push({
			path: `${base}.sampling_temperature`,
			message: SAMPLING_TEMPERATURE_ERROR,
		})
	}
	return problems
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

export const EVIDENCE_WEIGHT_KEYS = [
	'volume',
	'diversity',
	'environment',
	'strength',
] as const

export type EvidenceWeightKey = (typeof EVIDENCE_WEIGHT_KEYS)[number]

export interface EvidenceSettings {
	calibrationFloor: number | null
	minEvidence: number | null
	influence: number | null
	referenceScore: number | null
	weights: Record<EvidenceWeightKey, number | null>
}

function percentOf(
	source: JsonObject,
	key: string,
	fallback: number,
): number | null {
	if (!(key in source)) return Math.round(fallback * 100)
	const value = numberField(source, key)
	return value === null ? null : Math.round(value * 100)
}

export function evidenceSettingsOf(value: JsonObject): EvidenceSettings {
	const verdict = isJsonObject(value.verdict) ? value.verdict : {}
	const evidence = isJsonObject(value.evidence) ? value.evidence : {}
	const weights = isJsonObject(evidence.weights) ? evidence.weights : {}
	const weight = (key: EvidenceWeightKey) =>
		key in weights ? numberField(weights, key) : DEFAULT_EVIDENCE_WEIGHT
	return {
		calibrationFloor: percentOf(
			verdict,
			'calibration_floor',
			DEFAULT_CALIBRATION_FLOOR,
		),
		minEvidence: percentOf(verdict, 'min_evidence', DEFAULT_MIN_EVIDENCE),
		influence: percentOf(evidence, 'influence', DEFAULT_INFLUENCE),
		referenceScore:
			'reference_score' in evidence
				? numberField(evidence, 'reference_score')
				: DEFAULT_REFERENCE_SCORE,
		weights: {
			volume: weight('volume'),
			diversity: weight('diversity'),
			environment: weight('environment'),
			strength: weight('strength'),
		},
	}
}

export const FLOOR_ERROR = `Doit être un pourcentage entier de 0 à ${MAX_CALIBRATION_FLOOR_PERCENT}`
export const REFERENCE_SCORE_ERROR = "Doit être un nombre d'au moins 1"
export const WEIGHT_ERROR = `Doit être un entier de 0 à ${MAX_EVIDENCE_WEIGHT}`
export const NO_WEIGHT_ERROR = 'Au moins un poids doit être supérieur à 0'

function isWeight(value: number | null): value is number {
	return (
		value !== null &&
		Number.isInteger(value) &&
		value >= 0 &&
		value <= MAX_EVIDENCE_WEIGHT
	)
}

export function evidenceProblems(settings: EvidenceSettings): Problem[] {
	const problems: Problem[] = []
	const { calibrationFloor, minEvidence, influence, referenceScore } = settings
	if (
		!isPercent(calibrationFloor) ||
		calibrationFloor > MAX_CALIBRATION_FLOOR_PERCENT
	) {
		problems.push({ path: 'verdict.calibration_floor', message: FLOOR_ERROR })
	}
	if (!isPercent(minEvidence)) {
		problems.push({ path: 'verdict.min_evidence', message: PERCENT_ERROR })
	}
	if (!isPercent(influence)) {
		problems.push({ path: 'evidence.influence', message: PERCENT_ERROR })
	}
	if (referenceScore === null || referenceScore < 1) {
		problems.push({
			path: 'evidence.reference_score',
			message: REFERENCE_SCORE_ERROR,
		})
	}
	for (const key of EVIDENCE_WEIGHT_KEYS) {
		if (!isWeight(settings.weights[key])) {
			problems.push({
				path: `evidence.weights.${key}`,
				message: WEIGHT_ERROR,
			})
		}
	}
	if (
		problems.every((item) => !item.path.startsWith('evidence.weights')) &&
		EVIDENCE_WEIGHT_KEYS.every((key) => settings.weights[key] === 0)
	) {
		problems.push({ path: 'evidence.weights', message: NO_WEIGHT_ERROR })
	}
	return problems
}

export function isPercent(value: number | null): value is number {
	return value !== null && Number.isInteger(value) && value >= 0 && value <= 100
}

export const VERDICT_ORDER_ERROR =
	'« Fiable à partir de » doit être supérieur à « Écarté sous »'
export const PERCENT_ERROR = 'Doit être un pourcentage entier de 0 à 100'
export const EXCLUSION_PERCENT_ERROR =
	'Doit être un pourcentage entier de 50 à 100'
export const UNKNOWN_EXCLUDES_ERROR =
	"L'option « inconnu » ne peut pas exclure le lead"

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

function option(
	value: string,
	label: string,
	score: number,
	unknown = false,
): JsonObject {
	return { value, label, score, is_unknown: unknown, excludes: false }
}

function unknownOption(): JsonObject {
	return option(UNKNOWN_VALUE, UNKNOWN_LABEL, UNKNOWN_SCORE, true)
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
			unknownOption(),
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
	const created = option(`option-${counter}`, `Option ${counter}`, 0)
	const firstUnknown = options.findIndex((item) => isUnknownOption(item))
	const at = firstUnknown === -1 ? options.length : firstUnknown
	return toObject(
		setIn(
			value,
			['qualification_agents', agent, 'questions', question, 'options'],
			[...options.slice(0, at), created, ...options.slice(at)],
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
	const options = optionsOf(
		questionsOf(qualificationList(value)[agent])[question],
	)
	if (isUnknownOption(options[position])) return value
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
	key: 'reliable_from' | 'discard_below' | 'calibration_floor' | 'min_evidence',
	next: number | null,
): JsonObject {
	const current = isJsonObject(value.verdict) ? value.verdict : {}
	return { ...value, verdict: { ...current, [key]: next } }
}

export function setEscalationField(
	value: JsonObject,
	key: 'provider' | 'band_low' | 'band_high',
	next: string | number | null,
): JsonObject {
	const current = isJsonObject(value.escalation) ? value.escalation : {}
	return { ...value, escalation: { ...current, [key]: next } }
}

export function setEvidenceField(
	value: JsonObject,
	key: 'influence' | 'reference_score',
	next: number | null,
): JsonObject {
	const current = isJsonObject(value.evidence) ? value.evidence : {}
	return { ...value, evidence: { ...current, [key]: next } }
}

export function setEvidenceWeight(
	value: JsonObject,
	key: EvidenceWeightKey,
	next: number | null,
): JsonObject {
	const current = isJsonObject(value.evidence) ? value.evidence : {}
	const weights = isJsonObject(current.weights) ? current.weights : {}
	return {
		...value,
		evidence: { ...current, weights: { ...weights, [key]: next } },
	}
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
			evidence: {
				type: 'string',
				maxLength: MAX_EVIDENCE_CHARS,
				description:
					'Sentence copied verbatim from the documents that justifies the choice. May be empty only when the choice is the unknown option.',
			},
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
		required: ['type', 'choice', 'evidence', 'confidence', 'probabilities'],
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
      "evidence": "<phrase copiée mot pour mot depuis les documents>",
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
		problems.push(problem(path, 'Doit avoir de 2 à 11 options'))
	} else if (options.filter((item) => isUnknownOption(item)).length !== 1) {
		problems.push(problem(path, 'Doit avoir exactement une option « inconnu »'))
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
		if (isExcludingOption(item) && isUnknownOption(item)) {
			problems.push(problem(`${itemPath}.excludes`, UNKNOWN_EXCLUDES_ERROR))
		}
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
		const threshold = exclusionThresholdOf(agent)
		if (
			threshold === null ||
			!Number.isInteger(threshold) ||
			threshold < MIN_EXCLUSION_PERCENT ||
			threshold > 100
		) {
			problems.push(
				problem(`${base}.exclusion_threshold`, EXCLUSION_PERCENT_ERROR),
			)
		}
		problems.push(...samplingProblems(agent, base))
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
	problems.push(...escalationProblems(escalationOf(value)))
	problems.push(...verdictProblems(verdictOf(value)))
	problems.push(...evidenceProblems(evidenceSettingsOf(value)))
	return problems
}
