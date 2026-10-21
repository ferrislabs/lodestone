import type { JsonObject } from '#/lib/schema-form'

export const DEFAULT_WEIGHT = 5

export interface QualificationTemplate {
	id: string
	label: string
	description: string
	name: string
	prompt: string
	questionId: string
	wording: string
	options: readonly (readonly [string, string, number])[]
}

const BLANK_TEMPLATE: QualificationTemplate = {
	id: 'vide',
	label: 'Vide',
	description: 'Un agent à compléter : une question, deux options.',
	name: 'Nouvel agent',
	prompt: '',
	questionId: 'question-1',
	wording: '',
	options: [
		['option-1', 'Option 1', 1],
		['option-2', 'Option 2', 0],
	],
}

export const QUALIFICATION_TEMPLATES: readonly QualificationTemplate[] = [
	{
		id: 'pertinence',
		label: 'Pertinence',
		description: "Le lead correspond-il à l'offre et au client cible ?",
		name: 'Pertinence',
		prompt:
			'You judge whether a company is a relevant prospect for the offer described in the market context. Base your answers only on what the documents and signals state. Do not assume facts that are not written. Answer with the closest option.',
		questionId: 'pertinence',
		wording: "Ce lead est-il pertinent pour l'offre ?",
		options: [
			['forte', 'Forte', 1],
			['moyenne', 'Moyenne', 0.5],
			['faible', 'Faible', 0],
		],
	},
	{
		id: 'maturite-technique',
		label: 'Maturité technique',
		description:
			"Où en est l'organisation sur le sujet de l'offre : dette, SaaS ou inconnu ?",
		name: 'Maturité technique',
		prompt:
			'You assess how mature the organization is on the topic of the offer from the signals and documents. Ageing self-hosted software means high technical debt and a strong opportunity. A recent SaaS product means a weaker one. Answer unknown when nothing shows it.',
		questionId: 'maturite',
		wording:
			"Quelle est la maturité technique de l'organisation sur le sujet de l'offre ?",
		options: [
			['legacy_auto_heberge', 'Ancien, auto-hébergé', 1],
			['saas', 'SaaS', 0.7],
			['inconnu', 'Inconnue', 0.3],
		],
	},
	{
		id: 'type-organisation',
		label: "Type d'organisation",
		description: "Public, grande entreprise ou PME : qui est l'acheteur ?",
		name: "Type d'organisation",
		prompt:
			'You classify the organization by type from the documents. Public bodies buy through tenders, large companies run their own IT teams, small and medium businesses rarely do. Answer unknown when nothing shows it.',
		questionId: 'type',
		wording: "De quel type d'organisation s'agit-il ?",
		options: [
			['public', 'Organisation publique', 0.8],
			['grande_entreprise', 'Grande entreprise', 1],
			['pme', 'PME', 0.5],
			['inconnu', 'Inconnu', 0.3],
		],
	},
	BLANK_TEMPLATE,
]

export const DEFAULT_AGGREGATOR_PROMPT =
	'You write the summary of a lead for a B2B lead detection tool. You receive the lead, its signals, and the answers and scores of every qualification agent. Write a short synthesis, the verdict narrative, an approach angle and a message draft for the operator to review. Quote evidence verbatim from the documents. Never change the computed confidence or the verdict. Base your text only on what is provided and do not invent facts.'

export function findTemplate(id: string): QualificationTemplate {
	return (
		QUALIFICATION_TEMPLATES.find((candidate) => candidate.id === id) ??
		BLANK_TEMPLATE
	)
}

export function templateAgent(
	template: QualificationTemplate,
	id: string,
): JsonObject {
	return {
		id,
		name: template.name,
		enabled: true,
		provider_id: null,
		system_prompt: template.prompt,
		weight: DEFAULT_WEIGHT,
		questions: [
			{
				id: template.questionId,
				wording: template.wording,
				weight: DEFAULT_WEIGHT,
				options: template.options.map(([value, label, score]) => ({
					value,
					label,
					score,
				})),
			},
		],
	}
}
