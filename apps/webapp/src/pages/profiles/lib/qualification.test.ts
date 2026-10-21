import { describe, expect, it } from 'vitest'

import type { JsonObject } from '#/lib/schema-form'
import { newAgent } from '#/pages/profiles/lib/agents'
import {
	addOption,
	addQuestion,
	allAgentIds,
	contractSchema,
	DEFAULT_AGGREGATOR_PROMPT,
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
	setOptionField,
	setQuestionWording,
	VERDICT_ORDER_ERROR,
	validateQualification,
	verdictBands,
	verdictOf,
	verdictProblems,
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
	it('offers pertinence, technical maturity, organization type and blank', () => {
		expect(QUALIFICATION_TEMPLATES.map((template) => template.label)).toEqual([
			'Pertinence',
			'Maturité technique',
			"Type d'organisation",
			'Vide',
		])
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
			{ value: 'forte', label: 'Forte', score: 1 },
			{ value: 'moyenne', label: 'Moyenne', score: 0.5 },
			{ value: 'faible', label: 'Faible', score: 0 },
		])
	})

	it('builds the technical maturity and organization options', () => {
		const maturity = newQualificationAgent('maturite-technique', new Set())
		expect(optionsOf(questionsOf(maturity)[0]).map((item) => item)).toEqual([
			{ value: 'legacy_auto_heberge', label: 'Ancien, auto-hébergé', score: 1 },
			{ value: 'saas', label: 'SaaS', score: 0.7 },
			{ value: 'inconnu', label: 'Inconnue', score: 0.3 },
		])
		const kind = newQualificationAgent('type-organisation', new Set())
		expect(
			optionsOf(questionsOf(kind)[0]).map((item) =>
				typeof item === 'object' && item !== null && 'value' in item
					? item.value
					: null,
			),
		).toEqual(['public', 'grande_entreprise', 'pme', 'inconnu'])
	})

	it('builds a blank agent with one empty question and two options', () => {
		const agent = newQualificationAgent('vide', new Set())
		expect(agent.system_prompt).toBe('')
		expect(questionsOf(agent)).toHaveLength(1)
		expect(optionsOf(questionsOf(agent)[0])).toHaveLength(2)
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
		expect(optionsOf(questions[1])).toHaveLength(2)
		const grown = addOption(withQuestion, 0, 1)
		expect(optionsOf(questionsOf(qualificationList(grown)[0])[1])).toHaveLength(
			3,
		)
		const shrunk = removeOption(grown, 0, 1, 0)
		expect(
			optionsOf(questionsOf(qualificationList(shrunk)[0])[1]),
		).toHaveLength(2)
		const back = removeQuestion(shrunk, 0, 1)
		expect(questionsOf(qualificationList(back)[0])).toHaveLength(1)
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
			'confidence',
			'probabilities',
		])
		expect(question.additionalProperties).toBe(false)
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
			'qualification_agents[0].questions[0].options[0].score',
			'qualification_agents[0].questions[0].options[1].value',
			'qualification_agents[0].questions[1].id',
			'qualification_agents[0].questions[1].options',
		])
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
})
