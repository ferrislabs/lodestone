import { describe, expect, it } from 'vitest'

import type { JsonObject } from '#/lib/schema-form'
import { newAgent } from '#/pages/profiles/lib/agents'
import {
	addOption,
	addQuestion,
	allAgentIds,
	contractSchema,
	DEFAULT_AGGREGATOR_PROMPT,
	DEFAULT_HALF_LIVES,
	displayOrder,
	EVIDENCE_WEIGHT_KEYS,
	escalationOf,
	escalationProblems,
	evidenceProblems,
	evidenceSettingsOf,
	exclusionThresholdOf,
	FLOOR_ERROR,
	HALF_LIFE_ERROR,
	isExcludingOption,
	isUnknownOption,
	NO_WEIGHT_ERROR,
	newQualificationAgent,
	optionsOf,
	PERCENT_ERROR,
	QUALIFICATION_TEMPLATES,
	qualificationList,
	qualificationSummary,
	questionsOf,
	removeOption,
	removeQuestion,
	renameQualificationAgent,
	samplesOf,
	samplingTemperatureOf,
	setEscalationField,
	setEvidenceField,
	setEvidenceWeight,
	setHalfLife,
	setOptionField,
	setQuestionWording,
	setVerdictField,
	VERDICT_ORDER_ERROR,
	validateQualification,
	verdictBands,
	verdictOf,
	verdictProblems,
	WEIGHT_ERROR,
	withQualification,
} from '#/pages/profiles/lib/qualification'

function profile(): JsonObject {
	return {
		agents: [newAgent('ats', [])],
		qualification_agents: [newQualificationAgent('pertinence', new Set())],
		aggregator: { provider_id: null, system_prompt: DEFAULT_AGGREGATOR_PROMPT },
		verdict: { reliable_from: 70, discard_below: 30 },
	}
}

describe('templates', () => {
	it('offers the ready-made templates then blank', () => {
		expect(QUALIFICATION_TEMPLATES.map((template) => template.label)).toEqual([
			'Pertinence',
			'Maturité technique',
			'Environnement',
			"Type et taille d'organisation",
			'Marché',
			'Opportunité',
			'Recherche entreprise',
			'Exclusions',
			'Vide',
		])
	})

	it('builds templates that pass the v10 validation', () => {
		for (const template of QUALIFICATION_TEMPLATES) {
			if (template.id === 'vide') continue
			const agent = newQualificationAgent(template.id, new Set(['ats']))
			const value: JsonObject = {
				...profile(),
				qualification_agents: [agent],
			}
			expect(validateQualification(value), template.id).toEqual([])
			expect(String(agent.description).length).toBeLessThanOrEqual(300)
			expect(String(agent.system_prompt).length).toBeLessThanOrEqual(8000)
			expect(String(agent.system_prompt)).not.toBe('')
			const questions = questionsOf(agent)
			expect(questions.length).toBe(template.axes.length)
			for (const question of questions) {
				const wording = (question as JsonObject).wording as string
				expect(wording.length).toBeGreaterThan(0)
				const scores = optionsOf(question).map(
					(item) => (item as JsonObject).score as number,
				)
				expect(Math.max(...scores)).toBe(1)
				expect(Math.min(...scores)).toBeGreaterThanOrEqual(0)
			}
		}
	})

	it('builds the environment, market and opportunity axes', () => {
		const values = (id: string) =>
			questionsOf(newQualificationAgent(id, new Set())).map((question) => [
				(question as JsonObject).id,
				optionsOf(question).map((item) => (item as JsonObject).value),
			])
		expect(values('environnement')).toEqual([
			['environnement', ['production', 'mixte', 'hors_production', 'inconnu']],
			['maturite_usage', ['usage_reel', 'experimentation', 'inconnu']],
		])
		expect(values('marche')).toEqual([
			['zone', ['dans_la_zone', 'proche', 'hors_zone', 'inconnu']],
			['langue', ['francais', 'autre', 'inconnu']],
		])
		expect(values('opportunite')).toEqual([
			['migration', ['en_cours', 'probable', 'improbable', 'inconnu']],
			['declencheur', ['fort', 'moyen', 'aucun', 'inconnu']],
		])
		expect(newQualificationAgent('marche', new Set()).system_prompt).toContain(
			'Zone visée : France et Europe francophone.',
		)
	})

	it('builds a pertinence agent with the three scored options', () => {
		const agent = newQualificationAgent('pertinence', new Set())
		expect(agent).toMatchObject({
			id: 'pertinence',
			name: 'Pertinence',
			description: "La pertinence du lead pour l'offre du profil.",
			enabled: true,
			provider_id: null,
			weight: 5,
		})
		const [question] = questionsOf(agent)
		expect(question).toMatchObject({
			id: 'pertinence',
			wording: "Ce lead est-il pertinent pour l'offre ?",
			weight: 5,
		})
		expect(optionsOf(question)).toEqual([
			{
				value: 'forte',
				label: 'Forte',
				score: 1,
				is_unknown: false,
				excludes: false,
			},
			{
				value: 'moyenne',
				label: 'Moyenne',
				score: 0.5,
				is_unknown: false,
				excludes: false,
			},
			{
				value: 'faible',
				label: 'Faible',
				score: 0,
				is_unknown: false,
				excludes: false,
			},
			{
				value: 'inconnu',
				label: 'Inconnu',
				score: 0.5,
				is_unknown: true,
				excludes: false,
			},
		])
	})

	it('builds the exclusions agent with excluding options and a low weight', () => {
		const agent = newQualificationAgent('exclusions', new Set())
		expect(agent).toMatchObject({
			id: 'exclusions',
			name: 'Exclusions',
			enabled: true,
			weight: 1,
			exclusion_threshold: 0.6,
		})
		expect(String(agent.system_prompt)).toContain('Clients actuels :')
		expect(String(agent.system_prompt)).toContain('Concurrents :')
		expect(String(agent.system_prompt)).toContain(
			'Éditeurs et intégrateurs du domaine :',
		)
		expect(String(agent.system_prompt)).toContain('Okta')
		expect(String(agent.system_prompt)).toContain('Wavestone')
		const axes = questionsOf(agent).map((question) => [
			(question as JsonObject).id,
			optionsOf(question).map((item) => [
				(item as JsonObject).value,
				(item as JsonObject).score,
				(item as JsonObject).is_unknown,
				(item as JsonObject).excludes,
			]),
		])
		expect(axes).toEqual([
			[
				'editeur_integrateur',
				[
					['oui', 0, false, true],
					['non', 1, false, false],
					['inconnu', 0.5, true, false],
				],
			],
			[
				'hebergeur_agence',
				[
					['oui', 0, false, true],
					['non', 1, false, false],
					['inconnu', 0.5, true, false],
				],
			],
			[
				'domaine_inactif',
				[
					['parque_ou_inactif', 0, false, true],
					['actif', 1, false, false],
					['inconnu', 0.5, true, false],
				],
			],
			[
				'client_actuel',
				[
					['oui', 0, false, true],
					['non', 1, false, false],
					['inconnu', 0.5, true, false],
				],
			],
			[
				'concurrent',
				[
					['oui', 0, false, true],
					['non', 1, false, false],
					['inconnu', 0.5, true, false],
				],
			],
		])
		expect(
			validateQualification(withQualification(profile(), [agent])),
		).toEqual([])
	})

	it('leaves every other template without an excluding option', () => {
		for (const template of QUALIFICATION_TEMPLATES) {
			if (template.id === 'exclusions') continue
			const agent = newQualificationAgent(template.id, new Set())
			expect(agent.exclusion_threshold).toBe(0.6)
			for (const question of questionsOf(agent)) {
				expect(
					optionsOf(question).some((item) => isExcludingOption(item)),
				).toBe(false)
			}
		}
	})

	it('builds the technical maturity and organization options', () => {
		const maturity = newQualificationAgent('maturite-technique', new Set())
		expect(optionsOf(questionsOf(maturity)[0]).map((item) => item)).toEqual([
			{
				value: 'legacy_auto_heberge',
				label: 'Ancien, auto-hébergé',
				score: 1,
				is_unknown: false,
				excludes: false,
			},
			{
				value: 'saas',
				label: 'SaaS',
				score: 0.7,
				is_unknown: false,
				excludes: false,
			},
			{
				value: 'inconnu',
				label: 'Inconnue',
				score: 0.3,
				is_unknown: true,
				excludes: false,
			},
		])
		const kind = newQualificationAgent('type-taille-organisation', new Set())
		expect(
			optionsOf(questionsOf(kind)[0]).map((item) =>
				typeof item === 'object' && item !== null && 'value' in item
					? item.value
					: null,
			),
		).toEqual([
			'secteur_public',
			'grand_compte',
			'eti',
			'pme',
			'startup',
			'inconnu',
		])
	})

	it('builds a blank agent with one empty question, two options and the unknown option', () => {
		const agent = newQualificationAgent('vide', new Set())
		expect(agent.system_prompt).toBe('')
		expect(questionsOf(agent)).toHaveLength(1)
		const options = optionsOf(questionsOf(agent)[0])
		expect(options).toHaveLength(3)
		expect(options[2]).toMatchObject({ value: 'inconnu', is_unknown: true })
	})

	it('gives every template axis exactly one unknown option', () => {
		for (const template of QUALIFICATION_TEMPLATES) {
			const agent = newQualificationAgent(template.id, new Set())
			for (const question of questionsOf(agent)) {
				const unknown = optionsOf(question).filter((item) =>
					isUnknownOption(item),
				)
				expect(
					unknown,
					`${template.id}/${(question as JsonObject).id}`,
				).toHaveLength(1)
			}
		}
	})

	it('keeps ids unique across sourcing and qualification agents', () => {
		const value = profile()
		const taken = allAgentIds(value)
		expect(taken).toEqual(new Set(['ats', 'pertinence']))
		const second = newQualificationAgent('pertinence', taken)
		expect(second.id).toBe('pertinence-2')
		const clash = newQualificationAgent('vide', new Set(['nouvel-agent']))
		expect(clash.id).toBe('nouvel-agent-2')
	})
})

describe('agent and question edits', () => {
	it('derives the id from the name until it is customised', () => {
		const renamed = renameQualificationAgent(profile(), 0, 'Maturité IAM')
		expect(qualificationList(renamed)[0]).toMatchObject({
			name: 'Maturité IAM',
			id: 'maturite-iam',
		})
		const custom = withQualification(renamed, [
			{ ...(qualificationList(renamed)[0] as JsonObject), id: 'mon-id' },
		])
		const again = renameQualificationAgent(custom, 0, 'Autre')
		expect(qualificationList(again)[0]).toMatchObject({
			name: 'Autre',
			id: 'mon-id',
		})
	})

	it('does not reuse the id of a sourcing agent', () => {
		const value = {
			...profile(),
			agents: [{ ...newAgent('ats', []), id: 'autre' }],
		}
		const renamed = renameQualificationAgent(value, 0, 'Autre')
		expect(qualificationList(renamed)[0]).toMatchObject({ id: 'autre-2' })
	})

	it('adds and removes questions and options', () => {
		const withQuestion = addQuestion(profile(), 0)
		const questions = questionsOf(qualificationList(withQuestion)[0])
		expect(questions).toHaveLength(2)
		expect(questions[1]).toMatchObject({ id: 'question-2', wording: '' })
		expect(optionsOf(questions[1])).toHaveLength(3)
		const grown = addOption(withQuestion, 0, 1)
		const grownOptions = optionsOf(questionsOf(qualificationList(grown)[0])[1])
		expect(grownOptions).toHaveLength(4)
		expect(grownOptions.map((item) => (item as JsonObject).value)).toEqual([
			'option-1',
			'option-2',
			'option-4',
			'inconnu',
		])
		const shrunk = removeOption(grown, 0, 1, 0)
		expect(
			optionsOf(questionsOf(qualificationList(shrunk)[0])[1]),
		).toHaveLength(3)
		const back = removeQuestion(shrunk, 0, 1)
		expect(questionsOf(qualificationList(back)[0])).toHaveLength(1)
	})

	it('never removes the unknown option', () => {
		const unknownAt = optionsOf(
			questionsOf(qualificationList(profile())[0])[0],
		).findIndex((item) => isUnknownOption(item))
		expect(unknownAt).toBeGreaterThan(-1)
		const same = removeOption(profile(), 0, 0, unknownAt)
		expect(same).toEqual(profile())
	})

	it('keeps the unknown option editable on its score', () => {
		const unknownAt = optionsOf(
			questionsOf(qualificationList(profile())[0])[0],
		).findIndex((item) => isUnknownOption(item))
		const next = setOptionField(profile(), 0, 0, unknownAt, 'score', 0.2)
		expect(
			optionsOf(questionsOf(qualificationList(next)[0])[0])[unknownAt],
		).toMatchObject({ value: 'inconnu', score: 0.2, is_unknown: true })
	})

	it('orders the unknown option last for display', () => {
		expect(
			displayOrder([
				{ value: 'a', is_unknown: false },
				{ value: 'inconnu', is_unknown: true },
				{ value: 'b' },
			]),
		).toEqual([0, 2, 1])
	})

	it('edits an option score', () => {
		const next = setOptionField(profile(), 0, 0, 1, 'score', 0.4)
		expect(
			optionsOf(questionsOf(qualificationList(next)[0])[0])[1],
		).toMatchObject({ value: 'moyenne', score: 0.4 })
	})

	it('derives a generated question id from its wording', () => {
		const added = addQuestion(profile(), 0)
		const worded = setQuestionWording(added, 0, 1, 'Quel budget ?')
		expect(questionsOf(qualificationList(worded)[0])[1]).toMatchObject({
			wording: 'Quel budget ?',
			id: 'quel-budget',
		})
		const kept = setQuestionWording(worded, 0, 0, 'Autre libellé')
		expect(questionsOf(qualificationList(kept)[0])[0]).toMatchObject({
			id: 'pertinence',
		})
	})
})

describe('contractSchema', () => {
	const agent = newQualificationAgent('maturite-technique', new Set())
	const schema = contractSchema(agent)

	it('requires the summary and the answers and forbids extra keys', () => {
		expect(schema.required).toEqual(['summary', 'answers'])
		expect(schema.additionalProperties).toBe(false)
	})

	it('bounds the summary to 1 to 600 characters', () => {
		expect((schema.properties as JsonObject).summary).toEqual({
			type: 'string',
			minLength: 1,
			maxLength: 600,
		})
	})

	it('describes each question with its option values', () => {
		const answers = (schema.properties as JsonObject).answers as JsonObject
		expect(answers.required).toEqual(['maturite'])
		expect(answers.additionalProperties).toBe(false)
		const question = (answers.properties as JsonObject).maturite as JsonObject
		const properties = question.properties as JsonObject
		expect(properties.type).toEqual({ type: 'string', const: 'choice' })
		expect((properties.choice as JsonObject).enum).toEqual([
			'legacy_auto_heberge',
			'saas',
			'inconnu',
		])
		expect(properties.confidence).toEqual({
			type: 'number',
			minimum: 0,
			maximum: 1,
		})
		expect(question.required).toEqual([
			'type',
			'choice',
			'evidence',
			'confidence',
			'probabilities',
		])
		expect(question.additionalProperties).toBe(false)
	})

	it('requires a bounded verbatim evidence string on every answer', () => {
		const answers = (schema.properties as JsonObject).answers as JsonObject
		const question = (answers.properties as JsonObject).maturite as JsonObject
		const evidence = (question.properties as JsonObject).evidence as JsonObject
		expect(evidence.type).toBe('string')
		expect(evidence.maxLength).toBe(300)
		expect(String(evidence.description)).toContain('verbatim')
	})

	it('bounds the probabilities to exactly the option keys', () => {
		const answers = (schema.properties as JsonObject).answers as JsonObject
		const question = (answers.properties as JsonObject).maturite as JsonObject
		const probabilities = (question.properties as JsonObject)
			.probabilities as JsonObject
		expect(Object.keys(probabilities.properties as JsonObject)).toEqual([
			'legacy_auto_heberge',
			'saas',
			'inconnu',
		])
		expect(probabilities.required).toEqual([
			'legacy_auto_heberge',
			'saas',
			'inconnu',
		])
		expect(probabilities.additionalProperties).toBe(false)
	})
})

describe('summary', () => {
	const agent = newQualificationAgent('pertinence', new Set())

	it('counts questions, names the provider and shows the weight', () => {
		expect(qualificationSummary(agent, [])).toBe(
			"1 axe d'analyse · fournisseur : par défaut · poids 5",
		)
		const provider = { ...agent, provider_id: 'p1', weight: 8 }
		const two = addQuestion(withQualification(profile(), [provider]), 0)
		expect(
			qualificationSummary(qualificationList(two)[0], [
				{ id: 'p1', name: 'Ollama local', enabled: true },
			]),
		).toBe("2 axes d'analyse · fournisseur : Ollama local · poids 8")
		expect(qualificationSummary(provider, [])).toContain('introuvable')
	})
})

describe('verdict thresholds', () => {
	it('defaults to 70 and 30', () => {
		expect(verdictOf({})).toEqual({ reliableFrom: 70, discardBelow: 30 })
	})

	it('accepts ordered percentages and shows three bands', () => {
		const verdict = { reliableFrom: 70, discardBelow: 30 }
		expect(verdictProblems(verdict)).toEqual([])
		expect(verdictBands(verdict)).toEqual({
			discarded: 30,
			toCheck: 40,
			reliable: 30,
		})
	})

	it('rejects equal or inverted thresholds', () => {
		for (const verdict of [
			{ reliableFrom: 30, discardBelow: 30 },
			{ reliableFrom: 20, discardBelow: 40 },
		]) {
			expect(verdictProblems(verdict)).toEqual([
				{ path: 'verdict.reliable_from', message: VERDICT_ORDER_ERROR },
			])
			expect(verdictBands(verdict)).toBeNull()
		}
	})

	it('rejects empty, fractional and out of range values', () => {
		expect(verdictProblems({ reliableFrom: null, discardBelow: 30 })).toEqual([
			{ path: 'verdict.reliable_from', message: PERCENT_ERROR },
		])
		expect(verdictProblems({ reliableFrom: 70.5, discardBelow: 101 })).toEqual([
			{ path: 'verdict.reliable_from', message: PERCENT_ERROR },
			{ path: 'verdict.discard_below', message: PERCENT_ERROR },
		])
	})
})

describe('evidence settings', () => {
	it('defaults to the engine defaults when the sections are absent', () => {
		expect(evidenceSettingsOf({})).toEqual({
			calibrationFloor: 5,
			minEvidence: 50,
			influence: 60,
			weights: { volume: 1, diversity: 1, environment: 1, strength: 1 },
			halfLives: DEFAULT_HALF_LIVES,
		})
		expect(evidenceProblems(evidenceSettingsOf({}))).toEqual([])
	})

	it('reads the stored fractions as percentages', () => {
		const settings = evidenceSettingsOf({
			verdict: { calibration_floor: 0.1, min_evidence: 0.7 },
			evidence: {
				influence: 0.25,
				weights: { volume: 3, diversity: 0, environment: 2, strength: 1 },
				half_life_days: { job_offer: 30, default: 60 },
			},
		})
		expect(settings).toEqual({
			calibrationFloor: 10,
			minEvidence: 70,
			influence: 25,
			weights: { volume: 3, diversity: 0, environment: 2, strength: 1 },
			halfLives: { ...DEFAULT_HALF_LIVES, job_offer: 30, default: 60 },
		})
	})

	it('writes each field without touching its siblings', () => {
		const value = profile()
		const floor = setVerdictField(value, 'calibration_floor', 0.1)
		expect(floor.verdict).toEqual({
			reliable_from: 70,
			discard_below: 30,
			calibration_floor: 0.1,
		})
		const influence = setEvidenceField(floor, 'influence', 0.4)
		expect(influence.evidence).toEqual({ influence: 0.4 })
		const weight = setEvidenceWeight(influence, 'volume', 4)
		expect(weight.evidence).toEqual({ influence: 0.4, weights: { volume: 4 } })
		expect(setEvidenceWeight(weight, 'strength', 2).evidence).toEqual({
			influence: 0.4,
			weights: { volume: 4, strength: 2 },
		})
	})

	it('writes a half-life without touching the other kinds', () => {
		const first = setHalfLife(profile(), 'job_offer', 30)
		expect(first.evidence).toEqual({ half_life_days: { job_offer: 30 } })
		const second = setHalfLife(first, 'default', 45)
		expect(second.evidence).toEqual({
			half_life_days: { job_offer: 30, default: 45 },
		})
		expect(evidenceSettingsOf(second).halfLives.public_tender).toBe(180)
	})

	it('rejects half-lives that are not whole days from 1 to 3650', () => {
		const base = evidenceSettingsOf({})
		const problems = evidenceProblems({
			...base,
			halfLives: {
				...base.halfLives,
				job_offer: 0,
				public_tender: 3651,
				default: null,
				saas_provider: 1.5,
			},
		})
		expect(problems).toEqual([
			{ path: 'evidence.half_life_days.job_offer', message: HALF_LIFE_ERROR },
			{
				path: 'evidence.half_life_days.public_tender',
				message: HALF_LIFE_ERROR,
			},
			{
				path: 'evidence.half_life_days.saas_provider',
				message: HALF_LIFE_ERROR,
			},
			{ path: 'evidence.half_life_days.default', message: HALF_LIFE_ERROR },
		])
		expect(
			evidenceProblems({
				...base,
				halfLives: { ...base.halfLives, job_offer: 3650, default: 1 },
			}),
		).toEqual([])
	})

	it('rejects a floor above 20 % and an empty percentage', () => {
		const settings = {
			...evidenceSettingsOf({}),
			calibrationFloor: 25,
			minEvidence: null,
		}
		expect(evidenceProblems(settings)).toEqual([
			{ path: 'verdict.calibration_floor', message: FLOOR_ERROR },
			{ path: 'verdict.min_evidence', message: PERCENT_ERROR },
		])
	})

	it('rejects weights outside 0 to 10 and all-zero weights', () => {
		const base = evidenceSettingsOf({})
		expect(
			evidenceProblems({
				...base,
				weights: { ...base.weights, volume: 11, diversity: null },
			}),
		).toEqual([
			{ path: 'evidence.weights.volume', message: WEIGHT_ERROR },
			{ path: 'evidence.weights.diversity', message: WEIGHT_ERROR },
		])
		const zero = Object.fromEntries(
			EVIDENCE_WEIGHT_KEYS.map((key) => [key, 0]),
		) as typeof base.weights
		expect(evidenceProblems({ ...base, weights: zero })).toEqual([
			{ path: 'evidence.weights', message: NO_WEIGHT_ERROR },
		])
	})

	it('reports evidence problems through the qualification validation', () => {
		const value = {
			...profile(),
			evidence: { influence: 1.5 },
		}
		expect(validateQualification(value).map((item) => item.path)).toEqual([
			'evidence.influence',
		])
	})
})

describe('validateQualification', () => {
	const paths = (value: JsonObject) =>
		validateQualification(value).map((problem) => problem.path)

	it('accepts the default profile', () => {
		expect(validateQualification(profile())).toEqual([])
	})

	it('requires at least one qualification agent', () => {
		expect(paths(withQualification(profile(), []))).toEqual([
			'qualification_agents',
		])
	})

	it('bounds the description to 300 characters', () => {
		const base = newQualificationAgent('pertinence', new Set())
		const long = withQualification(profile(), [
			{ ...base, description: 'x'.repeat(301) },
		])
		expect(paths(long)).toEqual(['qualification_agents[0].description'])
		const exact = withQualification(profile(), [
			{ ...base, description: 'x'.repeat(300) },
		])
		expect(paths(exact)).toEqual([])
	})

	it('requires ids unique across both agent lists', () => {
		const value = withQualification(profile(), [
			{ ...newQualificationAgent('pertinence', new Set()), id: 'ats' },
		])
		expect(paths(value)).toEqual(['qualification_agents[0].id'])
	})

	it('checks questions, options and scores with their paths', () => {
		const base = newQualificationAgent('pertinence', new Set())
		const question = questionsOf(base)[0] as JsonObject
		const agent = {
			...base,
			questions: [
				{
					...question,
					options: [
						{ value: 'a', label: 'A', score: 2 },
						{ value: 'a', label: 'A', score: 0 },
					],
				},
				{ ...question, options: [{ value: 'a', label: 'A', score: 0 }] },
			],
		}
		expect(paths(withQualification(profile(), [agent]))).toEqual([
			'qualification_agents[0].questions[0].options',
			'qualification_agents[0].questions[0].options[0].score',
			'qualification_agents[0].questions[0].options[1].value',
			'qualification_agents[0].questions[1].id',
			'qualification_agents[0].questions[1].options',
		])
	})

	it('requires exactly one unknown option per axis', () => {
		const base = newQualificationAgent('pertinence', new Set())
		const question = questionsOf(base)[0] as JsonObject
		const withOptions = (options: JsonObject[]) =>
			withQualification(profile(), [
				{ ...base, questions: [{ ...question, options }] },
			])
		const known = [
			{ value: 'a', label: 'A', score: 1, is_unknown: false, excludes: false },
			{ value: 'b', label: 'B', score: 0, is_unknown: false, excludes: false },
		]
		const unknown = (value: string) => ({
			value,
			label: 'Inconnu',
			score: 0.5,
			is_unknown: true,
		})
		const path = 'qualification_agents[0].questions[0].options'
		expect(paths(withOptions([...known, unknown('inconnu')]))).toEqual([])
		expect(paths(withOptions(known))).toEqual([path])
		expect(
			paths(withOptions([...known, unknown('inconnu'), unknown('autre')])),
		).toEqual([path])
		const messages = validateQualification(withOptions(known)).map(
			(problem) => problem.message,
		)
		expect(messages).toEqual(['Doit avoir exactement une option « inconnu »'])
	})

	it('accepts eleven options and rejects twelve', () => {
		const base = newQualificationAgent('pertinence', new Set())
		const question = questionsOf(base)[0] as JsonObject
		const many = (count: number): JsonObject[] =>
			Array.from({ length: count }, (_, position) => ({
				value: `o${position}`,
				label: 'L',
				score: 0.5,
				is_unknown: position === 0,
			}))
		const check = (count: number) =>
			paths(
				withQualification(profile(), [
					{ ...base, questions: [{ ...question, options: many(count) }] },
				]),
			)
		expect(check(11)).toEqual([])
		expect(check(12)).toEqual(['qualification_agents[0].questions[0].options'])
	})

	it('limits prompts to 8000 characters', () => {
		const value = {
			...withQualification(profile(), [
				{
					...newQualificationAgent('pertinence', new Set()),
					system_prompt: 'x'.repeat(8001),
				},
			]),
			aggregator: { provider_id: null, system_prompt: 'x'.repeat(8001) },
		}
		expect(paths(value)).toEqual([
			'qualification_agents[0].system_prompt',
			'aggregator.system_prompt',
		])
	})

	it('reports threshold order at the verdict path', () => {
		const value = {
			...profile(),
			verdict: { reliable_from: 10, discard_below: 30 },
		}
		expect(paths(value)).toEqual(['verdict.reliable_from'])
	})

	it('validates the exclusion threshold as a percentage from 50 to 100', () => {
		const agent = newQualificationAgent('exclusions', new Set())
		const path = 'qualification_agents[0].exclusion_threshold'
		for (const bad of [0.49, 1.01, 0, null]) {
			const problems = validateQualification(
				withQualification(profile(), [{ ...agent, exclusion_threshold: bad }]),
			)
			expect(
				problems.filter((problem) => problem.path === path),
				String(bad),
			).toEqual([
				{ path, message: 'Doit être un pourcentage entier de 50 à 100' },
			])
		}
		for (const good of [0.5, 0.6, 1]) {
			const problems = validateQualification(
				withQualification(profile(), [{ ...agent, exclusion_threshold: good }]),
			)
			expect(problems).toEqual([])
		}
	})

	it('reads the threshold as a percentage with its default', () => {
		expect(exclusionThresholdOf({ exclusion_threshold: 0.75 })).toBe(75)
		expect(exclusionThresholdOf({})).toBe(60)
		expect(exclusionThresholdOf({ exclusion_threshold: null })).toBeNull()
	})

	it('gives every template agent three samples at 0.7', () => {
		for (const template of QUALIFICATION_TEMPLATES) {
			const agent = newQualificationAgent(template.id, new Set())
			expect(agent.samples, template.id).toBe(3)
			expect(agent.sampling_temperature, template.id).toBe(0.7)
		}
	})

	it('reads the sampling fields with their defaults', () => {
		expect(samplesOf({ samples: 5 })).toBe(5)
		expect(samplesOf({})).toBe(3)
		expect(samplesOf({ samples: null })).toBeNull()
		expect(samplingTemperatureOf({ sampling_temperature: 1.2 })).toBe(1.2)
		expect(samplingTemperatureOf({})).toBe(0.7)
		expect(samplingTemperatureOf({ sampling_temperature: null })).toBeNull()
	})

	it('validates the samples from 1 to 5', () => {
		const agent = newQualificationAgent('pertinence', new Set())
		const path = 'qualification_agents[0].samples'
		for (const bad of [0, 6, 2.5, null]) {
			const problems = validateQualification(
				withQualification(profile(), [{ ...agent, samples: bad }]),
			)
			expect(
				problems.filter((problem) => problem.path === path),
				String(bad),
			).toEqual([{ path, message: 'Doit être un entier de 1 à 5' }])
		}
		for (const good of [1, 3, 5]) {
			expect(
				validateQualification(
					withQualification(profile(), [{ ...agent, samples: good }]),
				),
			).toEqual([])
		}
	})

	it('validates the sampling temperature from 0 to 1.5', () => {
		const agent = newQualificationAgent('pertinence', new Set())
		const path = 'qualification_agents[0].sampling_temperature'
		for (const bad of [-0.1, 1.6, null]) {
			const problems = validateQualification(
				withQualification(profile(), [{ ...agent, sampling_temperature: bad }]),
			)
			expect(
				problems.filter((problem) => problem.path === path),
				String(bad),
			).toEqual([{ path, message: 'Doit être un nombre de 0 à 1.5' }])
		}
		for (const good of [0, 0.7, 1.5]) {
			expect(
				validateQualification(
					withQualification(profile(), [
						{ ...agent, sampling_temperature: good },
					]),
				),
			).toEqual([])
		}
	})

	it('reads the escalation with its defaults', () => {
		expect(escalationOf({})).toEqual({
			provider: '',
			bandLow: 40,
			bandHigh: 70,
		})
		expect(
			escalationOf({
				escalation: { provider: 'p1', band_low: 20, band_high: 60 },
			}),
		).toEqual({ provider: 'p1', bandLow: 20, bandHigh: 60 })
		expect(escalationOf({ escalation: { provider: null } }).provider).toBe('')
	})

	it('sets one escalation field and keeps the others', () => {
		const value = setEscalationField(
			{ escalation: { provider: null, band_low: 40, band_high: 70 } },
			'band_low',
			25,
		)
		expect(value.escalation).toEqual({
			provider: null,
			band_low: 25,
			band_high: 70,
		})
		expect(setEscalationField({}, 'provider', 'p1').escalation).toEqual({
			provider: 'p1',
		})
	})

	it('validates the grey band as ordered percentages', () => {
		expect(
			escalationProblems({ provider: '', bandLow: 40, bandHigh: 70 }),
		).toEqual([])
		expect(
			escalationProblems({ provider: '', bandLow: 70, bandHigh: 70 }),
		).toEqual([
			{
				path: 'escalation.band_high',
				message: "« Jusqu'à » doit être supérieur à « À partir de »",
			},
		])
		expect(
			escalationProblems({ provider: '', bandLow: null, bandHigh: 101 }).map(
				(problem) => problem.path,
			),
		).toEqual(['escalation.band_low', 'escalation.band_high'])
		expect(
			validateQualification({
				...profile(),
				escalation: { provider: null, band_low: 80, band_high: 20 },
			}).map((problem) => problem.path),
		).toEqual(['escalation.band_high'])
	})

	it('rejects an unknown option that excludes', () => {
		const base = newQualificationAgent('exclusions', new Set())
		const question = questionsOf(base)[0] as JsonObject
		const options = optionsOf(question).map((item) =>
			isUnknownOption(item)
				? { ...(item as JsonObject), excludes: true }
				: item,
		)
		const problems = validateQualification(
			withQualification(profile(), [
				{ ...base, questions: [{ ...question, options }] },
			]),
		)
		expect(problems).toEqual([
			{
				path: 'qualification_agents[0].questions[0].options[2].excludes',
				message: "L'option « inconnu » ne peut pas exclure le lead",
			},
		])
	})

	it('toggles the excluding flag of an option', () => {
		const value = withQualification(profile(), [
			newQualificationAgent('pertinence', new Set()),
		])
		const next = setOptionField(value, 0, 0, 2, 'excludes', true)
		const options = optionsOf(questionsOf(qualificationList(next)[0])[0])
		expect(options.map((item) => isExcludingOption(item))).toEqual([
			false,
			false,
			true,
			false,
		])
	})
})
