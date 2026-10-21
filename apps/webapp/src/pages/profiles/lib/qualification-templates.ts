import type { JsonObject } from '#/lib/schema-form'

export const DEFAULT_WEIGHT = 5

export interface QualificationTemplate {
	id: string
	label: string
	description: string
	name: string
	evaluates: string
	prompt: string
	questionId: string
	wording: string
	options: readonly (readonly [string, string, number])[]
}

const BLANK_TEMPLATE: QualificationTemplate = {
	id: 'vide',
	label: 'Vide',
	description: "Un agent à compléter : un axe d'analyse, deux options.",
	name: 'Nouvel agent',
	evaluates: '',
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
		evaluates: "La pertinence du lead pour l'offre du profil.",
		prompt:
			"Tu évalues si une entreprise est un prospect pertinent pour l'offre décrite dans le contexte de marché. Base tes réponses uniquement sur ce que disent les documents et les signaux. Ne suppose aucun fait qui n'est pas écrit. Réponds avec l'option la plus proche.",
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
		evaluates:
			'La maturité technique du lead : versions utilisées, dette visible et signes de migration.',
		prompt:
			"Tu évalues la maturité de l'organisation sur le sujet de l'offre à partir des signaux et des documents. Un logiciel auto-hébergé vieillissant signifie une dette technique élevée et une forte opportunité. Un produit SaaS récent signifie une opportunité plus faible. Réponds « inconnu » quand rien ne permet de trancher.",
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
		evaluates:
			"Le type d'organisation du lead : public, grande entreprise ou PME.",
		prompt:
			"Tu classes l'organisation par type à partir des documents. Les organismes publics achètent par appels d'offres, les grandes entreprises ont leurs propres équipes informatiques, les PME rarement. Réponds « inconnu » quand rien ne permet de trancher.",
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
	"Tu rédiges le résumé d'un lead pour un outil de détection de leads B2B. Tu reçois le lead, ses signaux, ainsi que les réponses et les scores de chaque agent de qualification. Rédige une courte synthèse, le récit du verdict, un angle d'approche et un brouillon de message que l'opérateur relira. Cite les preuves mot pour mot depuis les documents. Ne modifie jamais la confiance ni le verdict calculés. Base ton texte uniquement sur ce qui est fourni et n'invente aucun fait."

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
		description: template.evaluates,
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
