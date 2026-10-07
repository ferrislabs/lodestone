import { pluralize } from '#/lib/labels'
import {
	isJsonObject,
	type JsonObject,
	type JsonValue,
	type Problem,
} from '#/lib/schema-form'
import {
	agentField,
	agentList,
	slugify,
	uniqueId,
} from '#/pages/profiles/lib/agents'
import {
	agentProvider,
	agentWeight,
	allAgentIds,
	checkOptions,
	listField,
	MAX_DESCRIPTION_CHARS,
	MAX_EVIDENCE_CHARS,
	MAX_PROMPT_CHARS,
	MAX_SAMPLES,
	MAX_SAMPLING_TEMPERATURE,
	MIN_SAMPLES,
	numberField,
	optionsOf,
	type ProviderOption,
	providerName,
	qualificationList,
	SAMPLES_ERROR,
	SAMPLING_TEMPERATURE_ERROR,
	SCORING_SCOPE,
	scoringList,
	setAgentField,
	UNKNOWN_LABEL,
	UNKNOWN_SCORE,
	UNKNOWN_VALUE,
	withScoring,
} from '#/pages/profiles/lib/qualification'
import { DEFAULT_WEIGHT } from '#/pages/profiles/lib/qualification-templates'

export const DEFAULT_SCORING_SAMPLES = 1
export const DEFAULT_SCORING_TEMPERATURE = 0.7
export const MAX_REASONS = 5
export const MAX_REASON_CHARS = 200
export const MAX_SCORE = 100
export const VERDICT_VALUES = ['fiable', 'a_verifier', 'ecarte'] as const

const IDENTIFIER = /^[a-z0-9][a-z0-9_-]{0,62}$/
const SLUG = /^[a-z0-9][a-z0-9-]{0,62}$/
const FALLBACK_ID = 'agent-scoring'

export const DEFAULT_SCORING_PROMPT = `Tu décides du score et du verdict d'un lead pour un outil de détection de leads B2B. Tu reçois le lead, ses faits, ses signaux, le dossier de l'entreprise, le rapport de chaque agent de qualification (résumé, réponses avec citations, accord entre échantillons, exclusions) et des indicateurs calculés par le système. Pèse l'ensemble comme le ferait un commercial prudent. Réponds à chaque critère en citant mot pour mot une phrase des documents, ou choisis l'option inconnue quand rien ne la soutient. Donne un score de 0 à 100, la probabilité de chaque verdict et quelques raisons courtes. Base-toi uniquement sur ce qui est fourni.`

export function criteriaOf(agent: JsonValue | undefined): JsonValue[] {
	return listField(agent, 'criteria')
}

export function scoringSamplesOf(agent: JsonValue | undefined): number | null {
	if (!isJsonObject(agent)) return null
	if (!('samples' in agent)) return DEFAULT_SCORING_SAMPLES
	return numberField(agent, 'samples')
}

export function scoringTemperatureOf(
	agent: JsonValue | undefined,
): number | null {
	if (!isJsonObject(agent)) return null
	if (!('sampling_temperature' in agent)) return DEFAULT_SCORING_TEMPERATURE
	return numberField(agent, 'sampling_temperature')
}

function option(
	value: string,
	label: string,
	score: number,
	unknown = false,
): JsonObject {
	return { value, label, score, is_unknown: unknown, excludes: false }
}

function criterion(
	id: string,
	wording: string,
	options: readonly [string, string, number][],
): JsonObject {
	return {
		id,
		wording,
		weight: DEFAULT_WEIGHT,
		options: [
			...options.map(([value, label, score]) => option(value, label, score)),
			option(UNKNOWN_VALUE, UNKNOWN_LABEL, UNKNOWN_SCORE, true),
		],
	}
}

export function newScoringAgent(taken: ReadonlySet<string>): JsonObject {
	return {
		id: uniqueId('score-global', taken),
		name: 'Score global',
		description:
			'Décide du score et du verdict du lead à partir des rapports des agents de qualification.',
		enabled: true,
		provider_id: null,
		weight: DEFAULT_WEIGHT,
		system_prompt: DEFAULT_SCORING_PROMPT,
		criteria: [
			criterion(
				'adequation_offre',
				"Le lead correspond-il à la cible de l'offre ?",
				[
					['forte', 'Forte', 1],
					['moyenne', 'Moyenne', 0.5],
					['faible', 'Faible', 0],
				],
			),
			criterion(
				'solidite_preuves',
				'Les preuves rassemblées sont-elles solides et récentes ?',
				[
					['solide', 'Solide', 1],
					['partielle', 'Partielle', 0.5],
					['fragile', 'Fragile', 0],
				],
			),
			criterion(
				'priorite_commerciale',
				'Ce lead mérite-t-il d’être contacté en priorité ?',
				[
					['haute', 'Haute', 1],
					['normale', 'Normale', 0.5],
					['basse', 'Basse', 0],
				],
			),
		],
		samples: DEFAULT_SCORING_SAMPLES,
		sampling_temperature: DEFAULT_SCORING_TEMPERATURE,
	}
}

export function renameScoringAgent(
	value: JsonObject,
	index: number,
	name: string,
): JsonObject {
	const agents = scoringList(value)
	const agent = agents[index]
	if (!isJsonObject(agent)) return value
	const id = agentField(agent, 'id')
	const base = slugify(agentField(agent, 'name')) || FALLBACK_ID
	const untouched = id === base || id.replace(/-\d+$/, '') === base
	const next: JsonObject = { ...agent, name }
	if (untouched) {
		next.id = uniqueId(
			slugify(name) || FALLBACK_ID,
			allAgentIds(value, index, SCORING_SCOPE),
		)
	}
	return withScoring(
		value,
		agents.map((item, position) => (position === index ? next : item)),
	)
}

export function setScoringField(
	value: JsonObject,
	index: number,
	key: string,
	next: JsonValue,
): JsonObject {
	return setAgentField(value, index, key, next, SCORING_SCOPE)
}

export const MIN_AGENTS_FOR_WEIGHT = 2

export function weightsApply(agentCount: number): boolean {
	return agentCount >= MIN_AGENTS_FOR_WEIGHT
}

export function scoringSummary(
	agent: JsonValue | undefined,
	providers: readonly ProviderOption[],
	showWeight: boolean,
): string {
	const count = criteriaOf(agent).length
	const samples = scoringSamplesOf(agent) ?? DEFAULT_SCORING_SAMPLES
	return [
		`${count} ${pluralize(count, 'critère', 'critères')}`,
		`fournisseur : ${providerName(agentProvider(agent), providers)}`,
		...(showWeight ? [`poids ${agentWeight(agent)}`] : []),
		`${samples} ${pluralize(samples, 'échantillon', 'échantillons')}`,
	].join(' · ')
}

function probabilitySchema(): JsonObject {
	return { type: 'number', minimum: 0, maximum: 1 }
}

function criterionSchema(item: JsonValue): JsonObject {
	const values = optionsOf(item).map((entry) => agentField(entry, 'value'))
	return {
		description: agentField(item, 'wording'),
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
					values.map((entry) => [entry, probabilitySchema()]),
				),
				required: values,
				additionalProperties: false,
			},
		},
		required: ['type', 'choice', 'evidence', 'confidence', 'probabilities'],
		additionalProperties: false,
	}
}

export function imposedContract(): JsonObject {
	return {
		score: {
			type: 'integer',
			minimum: 0,
			maximum: MAX_SCORE,
			description:
				'Score of the lead from 0 (not worth contacting) to 100 (contact first).',
		},
		verdict: {
			type: 'object',
			properties: {
				choice: { type: 'string', enum: [...VERDICT_VALUES] },
				confidence: probabilitySchema(),
				probabilities: {
					type: 'object',
					properties: Object.fromEntries(
						VERDICT_VALUES.map((value) => [value, probabilitySchema()]),
					),
					required: [...VERDICT_VALUES],
					additionalProperties: false,
				},
			},
			required: ['choice', 'confidence', 'probabilities'],
			additionalProperties: false,
		},
		reasons: {
			type: 'array',
			minItems: 1,
			maxItems: MAX_REASONS,
			items: { type: 'string', minLength: 1, maxLength: MAX_REASON_CHARS },
		},
	}
}

export function criteriaContract(agent: JsonValue | undefined): JsonObject {
	const items = criteriaOf(agent)
	const ids = items.map((item) => agentField(item, 'id'))
	return {
		type: 'object',
		properties: Object.fromEntries(
			items.map((item, position) => [
				ids[position] ?? '',
				criterionSchema(item),
			]),
		),
		required: ids,
		additionalProperties: false,
	}
}

export function scoringContractSchema(
	agent: JsonValue | undefined,
): JsonObject {
	return {
		$schema: 'https://json-schema.org/draft/2020-12/schema',
		title: `Score and verdict of ${agentField(agent, 'name')}`,
		type: 'object',
		properties: { criteria: criteriaContract(agent), ...imposedContract() },
		required: ['criteria', 'score', 'verdict', 'reasons'],
		additionalProperties: false,
	}
}

export const SCORING_CONTRACT_EXAMPLE = `{
  "criteria": {
    "<identifiant du critère>": {
      "type": "choice",
      "choice": "<valeur de l'option>",
      "evidence": "<phrase copiée mot pour mot depuis les documents>",
      "confidence": 0.0,
      "probabilities": { "<valeur de l'option>": 0.0 }
    }
  },
  "score": 0,
  "verdict": {
    "choice": "fiable | a_verifier | ecarte",
    "confidence": 0.0,
    "probabilities": { "fiable": 0.0, "a_verifier": 0.0, "ecarte": 0.0 }
  },
  "reasons": ["<1 à 5 phrases courtes>"]
}`

function problem(path: string, message: string): Problem {
	return { path, message }
}

const CRITERION_CHARS =
	'Doit être composé de minuscules, chiffres, tirets et tirets bas'

function checkCriteria(
	items: readonly JsonValue[],
	base: string,
	problems: Problem[],
) {
	const path = `${base}.criteria`
	if (items.length === 0) {
		problems.push(problem(path, 'Doit contenir au moins un critère'))
		return
	}
	const seen = new Set<string>()
	items.forEach((item, index) => {
		const itemPath = `${path}[${index}]`
		const id = agentField(item, 'id')
		if (id !== '' && !IDENTIFIER.test(id)) {
			problems.push(problem(`${itemPath}.id`, CRITERION_CHARS))
		} else if (id !== '' && seen.has(id)) {
			problems.push(problem(`${itemPath}.id`, "Doit être unique dans l'agent"))
		}
		seen.add(id)
		checkOptions(optionsOf(item), itemPath, problems, 'le critère')
		optionsOf(item).forEach((entry, position) => {
			if (isJsonObject(entry) && entry.excludes === true) {
				problems.push(
					problem(
						`${itemPath}.options[${position}].excludes`,
						'Un critère de scoring ne peut pas exclure le lead',
					),
				)
			}
		})
	})
}

export function scoringSamplingProblems(
	agent: JsonValue | undefined,
	base: string,
): Problem[] {
	const problems: Problem[] = []
	const samples = scoringSamplesOf(agent)
	if (
		samples === null ||
		!Number.isInteger(samples) ||
		samples < MIN_SAMPLES ||
		samples > MAX_SAMPLES
	) {
		problems.push(problem(`${base}.samples`, SAMPLES_ERROR))
	}
	const temperature = scoringTemperatureOf(agent)
	if (
		temperature === null ||
		temperature < 0 ||
		temperature > MAX_SAMPLING_TEMPERATURE
	) {
		problems.push(
			problem(`${base}.sampling_temperature`, SAMPLING_TEMPERATURE_ERROR),
		)
	}
	return problems
}

export function validateScoringAgents(value: JsonObject): Problem[] {
	const problems: Problem[] = []
	const seen = new Set<string>(
		[...agentList(value), ...qualificationList(value)].map((agent) =>
			agentField(agent, 'id'),
		),
	)
	scoringList(value).forEach((agent, index) => {
		const base = `scoring_agents[${index}]`
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
		if (agentField(agent, 'description').length > MAX_DESCRIPTION_CHARS) {
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
		problems.push(...scoringSamplingProblems(agent, base))
		checkCriteria(criteriaOf(agent), base, problems)
	})
	return problems
}
