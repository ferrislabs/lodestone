import { describe, expect, it } from 'vitest'

import type { JsonObject } from '#/lib/schema-form'
import {
	addOption,
	addQuestion,
	allAgentIds,
	isUnknownOption,
	newQualificationAgent,
	optionsOf,
	removeQuestion,
	SCORING_SCOPE,
	scoringList,
	setOptionField,
	setQuestionWording,
	withScoring,
} from '#/pages/profiles/lib/qualification'
import {
	criteriaContract,
	criteriaOf,
	DEFAULT_SCORING_PROMPT,
	imposedContract,
	newScoringAgent,
	renameScoringAgent,
	SCORING_CONTRACT_EXAMPLE,
	scoringContractSchema,
	scoringSamplesOf,
	scoringSummary,
	scoringTemperatureOf,
	setScoringField,
	validateScoringAgents,
	weightsApply,
} from '#/pages/profiles/lib/scoring-agents'

function profile(): JsonObject {
	return {
		agents: [
			{ id: 'ats', name: 'ATS', source: { kind: 'ats', keywords: ['a'] } },
		],
		qualification_agents: [
			newQualificationAgent('pertinence', new Set(['ats'])),
		],
		scoring_agents: [newScoringAgent(new Set(['ats', 'pertinence']))],
	}
}

function paths(value: JsonObject): string[] {
	return validateScoringAgents(value).map((problem) => problem.path)
}

describe('newScoringAgent', () => {
	it('builds the default agent with three criteria and an unknown option each', () => {
		const agent = newScoringAgent(new Set())
		expect(agent).toMatchObject({
			id: 'score-global',
			name: 'Score global',
			enabled: true,
			provider_id: null,
			weight: 5,
			samples: 1,
			sampling_temperature: 0.7,
			system_prompt: DEFAULT_SCORING_PROMPT,
		})
		expect(criteriaOf(agent).map((item) => (item as JsonObject).id)).toEqual([
			'adequation_offre',
			'solidite_preuves',
			'priorite_commerciale',
		])
		for (const criterion of criteriaOf(agent)) {
			const options = optionsOf(criterion)
			expect(options.filter((option) => isUnknownOption(option))).toHaveLength(
				1,
			)
			expect(
				options.every((option) => (option as JsonObject).excludes === false),
			).toBe(true)
		}
		expect(DEFAULT_SCORING_PROMPT.startsWith('Tu décides du score')).toBe(true)
	})

	it('takes an id unique among every kind of agent', () => {
		expect(newScoringAgent(new Set(['score-global'])).id).toBe('score-global-2')
	})

	it('is valid as it is', () => {
		expect(paths(profile())).toEqual([])
	})
})

describe('agent ids', () => {
	it('count the scoring agents next to the sourcing and qualification ones', () => {
		const ids = allAgentIds(profile())
		expect([...ids].sort()).toEqual(['ats', 'pertinence', 'score-global'])
		expect(allAgentIds(profile(), 0, SCORING_SCOPE).has('score-global')).toBe(
			false,
		)
		expect(allAgentIds(profile(), 0).has('score-global')).toBe(true)
	})

	it('follow the name of a scoring agent while untouched, uniquely', () => {
		const value = renameScoringAgent(profile(), 0, 'Décideur prudent')
		expect(scoringList(value)[0]).toMatchObject({
			name: 'Décideur prudent',
			id: 'decideur-prudent',
		})
		const custom = setScoringField(profile(), 0, 'id', 'mon-id')
		expect(
			scoringList(renameScoringAgent(custom, 0, 'Autre nom'))[0],
		).toMatchObject({
			id: 'mon-id',
		})
		const clash = withScoring(profile(), [
			...scoringList(profile()),
			{ ...newScoringAgent(new Set()), id: 'pertinence', name: 'Pertinence' },
		])
		const renamed = renameScoringAgent(clash, 0, 'Pertinence')
		expect(scoringList(renamed)[0]).toMatchObject({ id: 'pertinence-2' })
	})
})

describe('scoring defaults', () => {
	it('reads one sample and the default temperature when absent', () => {
		expect(scoringSamplesOf({})).toBe(1)
		expect(scoringTemperatureOf({})).toBe(0.7)
		expect(scoringSamplesOf({ samples: 4 })).toBe(4)
		expect(scoringSamplesOf({ samples: null })).toBeNull()
	})

	it('summarises criteria, provider, weight and samples', () => {
		const providers = [{ id: 'p', name: 'Laya', enabled: true }]
		const agent = {
			...newScoringAgent(new Set()),
			provider_id: 'p',
			samples: 3,
		}
		expect(scoringSummary(agent, providers, true)).toBe(
			'3 critères · fournisseur : Laya · poids 5 · 3 échantillons',
		)
		expect(scoringSummary(newScoringAgent(new Set()), [], true)).toBe(
			'3 critères · fournisseur : par défaut · poids 5 · 1 échantillon',
		)
	})

	it('leaves the weight out of the summary unless asked', () => {
		expect(scoringSummary(newScoringAgent(new Set()), [], false)).toBe(
			'3 critères · fournisseur : par défaut · 1 échantillon',
		)
	})

	it('applies weights from two scoring agents', () => {
		expect(weightsApply(0)).toBe(false)
		expect(weightsApply(1)).toBe(false)
		expect(weightsApply(2)).toBe(true)
	})
})

describe('criteria editing reuses the axis helpers on the criteria list', () => {
	it('adds, words and removes a criterion without touching qualification questions', () => {
		const added = addQuestion(profile(), 0, SCORING_SCOPE)
		const criteria = criteriaOf(scoringList(added)[0])
		expect(criteria).toHaveLength(4)
		expect(criteria[3]).toMatchObject({ id: 'question-4', wording: '' })
		expect(
			(added.qualification_agents as JsonObject[])[0]?.questions,
		).toHaveLength(1)
		const worded = setQuestionWording(
			added,
			0,
			3,
			'Budget disponible ?',
			SCORING_SCOPE,
		)
		expect(criteriaOf(scoringList(worded)[0])[3]).toMatchObject({
			wording: 'Budget disponible ?',
			id: 'budget-disponible',
		})
		const removed = removeQuestion(worded, 0, 3, SCORING_SCOPE)
		expect(criteriaOf(scoringList(removed)[0])).toHaveLength(3)
	})

	it('adds an option before the unknown one and edits its score', () => {
		const added = addOption(profile(), 0, 0, SCORING_SCOPE)
		const options = optionsOf(criteriaOf(scoringList(added)[0])[0])
		expect(options).toHaveLength(5)
		expect(isUnknownOption(options.at(-1))).toBe(true)
		const edited = setOptionField(added, 0, 0, 0, 'score', 0.8, SCORING_SCOPE)
		expect(optionsOf(criteriaOf(scoringList(edited)[0])[0])[0]).toMatchObject({
			score: 0.8,
		})
	})
})

describe('scoring contract', () => {
	it('imposes the score, the verdict and the reasons', () => {
		const imposed = imposedContract()
		expect(Object.keys(imposed)).toEqual(['score', 'verdict', 'reasons'])
		expect(imposed.score).toMatchObject({
			type: 'integer',
			minimum: 0,
			maximum: 100,
		})
		const verdict = imposed.verdict as JsonObject
		expect((verdict.properties as JsonObject).choice).toMatchObject({
			enum: ['fiable', 'a_verifier', 'ecarte'],
		})
		expect(verdict.additionalProperties).toBe(false)
		expect(imposed.reasons).toMatchObject({
			type: 'array',
			minItems: 1,
			maxItems: 5,
			items: { type: 'string', minLength: 1, maxLength: 200 },
		})
	})

	it('builds the criteria part from the criteria of the agent', () => {
		const agent = scoringList(profile())[0]
		const contract = criteriaContract(agent)
		expect(contract.required).toEqual([
			'adequation_offre',
			'solidite_preuves',
			'priorite_commerciale',
		])
		const first = (contract.properties as JsonObject)
			.adequation_offre as JsonObject
		expect(first.description).toBe(
			"Le lead correspond-il à la cible de l'offre ?",
		)
		expect(
			((first.properties as JsonObject).choice as JsonObject).enum,
		).toEqual(['forte', 'moyenne', 'faible', 'inconnu'])
		expect(first.required).toEqual([
			'type',
			'choice',
			'evidence',
			'confidence',
			'probabilities',
		])
	})

	it('assembles the full schema like the server does', () => {
		const schema = scoringContractSchema(scoringList(profile())[0])
		expect(schema.required).toEqual(['criteria', 'score', 'verdict', 'reasons'])
		expect(schema.additionalProperties).toBe(false)
		expect(Object.keys(schema.properties as JsonObject)).toEqual([
			'criteria',
			'score',
			'verdict',
			'reasons',
		])
		expect(schema.title).toBe('Score and verdict of Score global')
	})

	it('documents the shape in French', () => {
		for (const word of ['"criteria"', '"score"', '"verdict"', '"reasons"']) {
			expect(SCORING_CONTRACT_EXAMPLE).toContain(word)
		}
		expect(SCORING_CONTRACT_EXAMPLE).toContain('copiée mot pour mot')
	})
})

describe('validateScoringAgents', () => {
	it('accepts a profile without scoring agent', () => {
		expect(paths({ ...profile(), scoring_agents: [] })).toEqual([])
		const bare = profile()
		delete bare.scoring_agents
		expect(paths(bare)).toEqual([])
	})

	it('rejects an id that is not a slug or that another agent already uses', () => {
		expect(paths(setScoringField(profile(), 0, 'id', 'Mauvais Id'))).toEqual([
			'scoring_agents[0].id',
		])
		expect(paths(setScoringField(profile(), 0, 'id', 'pertinence'))).toEqual([
			'scoring_agents[0].id',
		])
		expect(paths(setScoringField(profile(), 0, 'id', 'ats'))).toEqual([
			'scoring_agents[0].id',
		])
	})

	it('bounds the texts, the samples and the temperature', () => {
		expect(
			paths(setScoringField(profile(), 0, 'description', 'x'.repeat(301))),
		).toEqual(['scoring_agents[0].description'])
		expect(
			paths(setScoringField(profile(), 0, 'system_prompt', 'x'.repeat(8001))),
		).toEqual(['scoring_agents[0].system_prompt'])
		for (const samples of [0, 6, 1.5, null]) {
			expect(paths(setScoringField(profile(), 0, 'samples', samples))).toEqual([
				'scoring_agents[0].samples',
			])
		}
		expect(
			paths(setScoringField(profile(), 0, 'sampling_temperature', 1.6)),
		).toEqual(['scoring_agents[0].sampling_temperature'])
		expect(paths(setScoringField(profile(), 0, 'samples', 5))).toEqual([])
	})

	it('needs at least one criterion with valid options', () => {
		expect(paths(setScoringField(profile(), 0, 'criteria', []))).toEqual([
			'scoring_agents[0].criteria',
		])
		const duplicated = removeQuestion(
			addQuestion(profile(), 0, SCORING_SCOPE),
			0,
			3,
			SCORING_SCOPE,
		)
		expect(paths(duplicated)).toEqual([])
		const unknownLess = setOptionField(
			profile(),
			0,
			0,
			3,
			'is_unknown',
			false,
			SCORING_SCOPE,
		)
		expect(paths(unknownLess)).toEqual([
			'scoring_agents[0].criteria[0].options',
		])
	})

	it('rejects a criterion option that excludes the lead', () => {
		const excluding = setOptionField(
			profile(),
			0,
			1,
			0,
			'excludes',
			true,
			SCORING_SCOPE,
		)
		expect(validateScoringAgents(excluding)).toEqual([
			{
				path: 'scoring_agents[0].criteria[1].options[0].excludes',
				message: 'Un critère de scoring ne peut pas exclure le lead',
			},
		])
	})
})
