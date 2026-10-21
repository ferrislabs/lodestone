import type { JsonObject } from '#/lib/schema-form'

export const DEFAULT_WEIGHT = 5
export const DEFAULT_EXCLUSION_THRESHOLD = 0.6

export type TemplateFlag = 'unknown' | 'excludes'

export type TemplateOption = readonly [string, string, number, TemplateFlag?]

export interface QualificationAxis {
	id: string
	wording: string
	weight?: number
	options: readonly TemplateOption[]
}

export interface QualificationTemplate {
	id: string
	label: string
	description: string
	name: string
	evaluates: string
	prompt: string
	weight?: number
	axes: readonly QualificationAxis[]
}

const BLANK_TEMPLATE: QualificationTemplate = {
	id: 'vide',
	label: 'Vide',
	description:
		"Un agent à compléter : un axe d'analyse, deux options et l'option « inconnu ».",
	name: 'Nouvel agent',
	evaluates: '',
	prompt: '',
	axes: [
		{
			id: 'question-1',
			wording: '',
			options: [
				['option-1', 'Option 1', 1],
				['option-2', 'Option 2', 0],
				['inconnu', 'Inconnu', 0.5, 'unknown'],
			],
		},
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
		axes: [
			{
				id: 'pertinence',
				wording: "Ce lead est-il pertinent pour l'offre ?",
				options: [
					['forte', 'Forte', 1],
					['moyenne', 'Moyenne', 0.5],
					['faible', 'Faible', 0],
					['inconnu', 'Inconnu', 0.5, 'unknown'],
				],
			},
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
		axes: [
			{
				id: 'maturite',
				wording:
					"Quelle est la maturité technique de l'organisation sur le sujet de l'offre ?",
				options: [
					['legacy_auto_heberge', 'Ancien, auto-hébergé', 1],
					['saas', 'SaaS', 0.7],
					['inconnu', 'Inconnue', 0.3, 'unknown'],
				],
			},
		],
	},
	{
		id: 'environnement',
		label: 'Environnement',
		description:
			'Les preuves viennent-elles de systèmes en production ou de dev, test et démo ?',
		name: 'Environnement',
		evaluates:
			"L'environnement des preuves : systèmes en production ou environnements de dev, test et démo, et réalité de l'usage.",
		prompt:
			"Tu évalues si les preuves d'un lead viennent de systèmes réellement utilisés. Appuie-toi sur le bloc « Faits » (environnement par hôte, part de production, sources) et sur les documents. Des hôtes de production, une instance publique stable ou des offres d'emploi qui parlent d'exploitation indiquent un usage réel. Des hôtes nommés dev, test, staging ou demo, un projet de démonstration ou un dépôt d'exemple indiquent de la non-production. Ne déduis pas l'environnement d'un simple nom de domaine sans indice. Réponds « inconnu » quand rien ne permet de trancher.",
		axes: [
			{
				id: 'environnement',
				wording:
					'Les preuves viennent-elles de systèmes en production ou hors production ?',
				options: [
					['production', 'Production', 1],
					['mixte', 'Mixte', 0.6],
					['hors_production', 'Hors production', 0],
					['inconnu', 'Inconnu', 0.3, 'unknown'],
				],
			},
			{
				id: 'maturite_usage',
				wording: "Quel est le niveau d'usage réel constaté ?",
				options: [
					['usage_reel', 'Usage réel', 1],
					['experimentation', 'Expérimentation', 0.4],
					['inconnu', 'Inconnu', 0.3, 'unknown'],
				],
			},
		],
	},
	{
		id: 'type-taille-organisation',
		label: "Type et taille d'organisation",
		description:
			"Secteur public, grand compte, ETI, PME ou startup : qui est l'acheteur ?",
		name: "Type et taille d'organisation",
		evaluates:
			"Le type et la taille de l'organisation : secteur public, grand compte, ETI, PME ou startup.",
		prompt:
			"Tu classes l'organisation par type et par taille à partir des documents et du bloc « Faits ». Le secteur public achète par appels d'offres, un grand compte a ses équipes et ses processus d'achat, une ETI décide plus vite, une PME ou une startup a peu de moyens dédiés. Appuie-toi sur ce que les documents disent de l'effectif, du chiffre d'affaires, du statut ou du secteur. Ne devine pas la taille d'après le nom. Réponds « inconnu » quand rien ne permet de trancher. L'opérateur ajuste les scores selon sa cible.",
		axes: [
			{
				id: 'type',
				wording: "De quel type d'organisation s'agit-il ?",
				options: [
					['secteur_public', 'Secteur public', 0.7],
					['grand_compte', 'Grand compte', 1],
					['eti', 'ETI', 0.8],
					['pme', 'PME', 0.5],
					['startup', 'Startup', 0.4],
					['inconnu', 'Inconnu', 0.3, 'unknown'],
				],
			},
			{
				id: 'taille',
				wording: "Quelle est la taille de l'organisation ?",
				options: [
					['grande', 'Grande', 1],
					['moyenne', 'Moyenne', 0.6],
					['petite', 'Petite', 0.3],
					['inconnue', 'Inconnue', 0.3, 'unknown'],
				],
			},
		],
	},
	{
		id: 'marche',
		label: 'Marché',
		description:
			"L'entreprise est-elle dans la zone visée, et dans la bonne langue ?",
		name: 'Marché',
		evaluates:
			"L'adéquation au marché visé : zone géographique et langue de l'entreprise.",
		prompt:
			"Zone visée : France et Europe francophone.\n\nTu évalues si l'entreprise appartient à la zone visée ci-dessus et si elle travaille dans la langue du marché. Appuie-toi sur le pays déduit des domaines dans le bloc « Faits » (extension de pays), sur l'adresse, la langue des documents et les offres d'emploi. Une extension de pays seule est un indice, pas une preuve : une extension .com ne dit rien. Une entreprise voisine de la zone, ou présente dans la zone par une filiale, est « proche ». Réponds « inconnu » quand rien ne permet de trancher. Modifie la première phrase pour changer de zone.",
		axes: [
			{
				id: 'zone',
				wording: "Où se situe l'entreprise par rapport à la zone visée ?",
				options: [
					['dans_la_zone', 'Dans la zone', 1],
					['proche', 'Proche de la zone', 0.5],
					['hors_zone', 'Hors zone', 0],
					['inconnu', 'Inconnu', 0.3, 'unknown'],
				],
			},
			{
				id: 'langue',
				wording: "Dans quelle langue l'entreprise travaille-t-elle ?",
				options: [
					['francais', 'Français', 1],
					['autre', 'Autre langue', 0.3],
					['inconnu', 'Inconnue', 0.3, 'unknown'],
				],
			},
		],
	},
	{
		id: 'opportunite',
		label: 'Opportunité',
		description:
			'Migration probable, dette technique, limites du SaaS actuel, recrutement.',
		name: 'Opportunité',
		evaluates:
			"Le potentiel d'opportunité : migration, dette technique, coût ou limites du SaaS actuel, recrutement sur le sujet.",
		prompt:
			"Tu évalues si le lead a une raison d'agir maintenant, à partir des signaux et des documents. Cherche une migration annoncée ou probable, une dette technique visible (versions anciennes, fin de support, contournements), le coût ou les limites du SaaS actuel (prix, dépendance, conformité, hébergement des données) et des offres d'emploi sur le sujet de l'offre. Un projet de migration en cours ou un déclencheur explicite est fort. L'absence de signal n'est pas un signal négatif : réponds « inconnu » ou « aucun » selon ce que disent les documents, sans rien supposer.",
		axes: [
			{
				id: 'migration',
				wording: 'Une migration est-elle en cours ou probable ?',
				options: [
					['en_cours', 'En cours', 1],
					['probable', 'Probable', 0.7],
					['improbable', 'Improbable', 0],
					['inconnu', 'Inconnue', 0.3, 'unknown'],
				],
			},
			{
				id: 'declencheur',
				wording: "Existe-t-il un déclencheur d'achat visible ?",
				options: [
					['fort', 'Fort', 1],
					['moyen', 'Moyen', 0.5],
					['aucun', 'Aucun', 0],
					['inconnu', 'Inconnu', 0.5, 'unknown'],
				],
			},
		],
	},
	{
		id: 'recherche-entreprise',
		label: 'Recherche entreprise',
		description:
			"Que fait l'entreprise et quelle taille a-t-elle ? Selon sa fiche officielle et son site.",
		name: 'Recherche entreprise',
		evaluates:
			"Ce que fait l'entreprise et sa taille, d'après sa fiche entreprise : données officielles du registre et pages de son site.",
		prompt:
			"Tu évalues l'entreprise à partir de sa fiche entreprise. Les documents « Company dossier » contiennent les données officielles du registre français (raison sociale, activité NAF, tranche d'effectif, siège, date de création) et le texte de pages du site de l'entreprise (accueil, à propos, mentions légales). Les données officielles sont fiables ; le texte du site est ce que l'entreprise dit d'elle-même. Appuie-toi uniquement sur cette fiche et sur le bloc « Faits ». Si la fiche est absente, ou si elle indique que le SIREN est introuvable, réponds « inconnu » : ne devine ni l'activité ni l'effectif d'après le nom de domaine. Évalue si l'activité de l'entreprise est pertinente pour l'offre, si sa taille correspond à la cible et si elle a de vrais enjeux d'identité et d'accès (clients, collaborateurs ou partenaires à authentifier, données sensibles, conformité). Ajuste la cible de taille dans ce prompt selon ton marché.",
		axes: [
			{
				id: 'activite_pertinente',
				wording:
					"L'activité de l'entreprise est-elle pertinente pour l'offre ?",
				options: [
					['oui', 'Oui', 1],
					['partiellement', 'Partiellement', 0.5],
					['non', 'Non', 0],
					['inconnu', 'Inconnue', 0.3, 'unknown'],
				],
			},
			{
				id: 'taille_cible',
				wording: "La taille de l'entreprise correspond-elle à la cible ?",
				weight: 4,
				options: [
					['dans_la_cible', 'Dans la cible', 1],
					['hors_cible', 'Hors cible', 0],
					['inconnu', 'Inconnue', 0.3, 'unknown'],
				],
			},
			{
				id: 'enjeux_identite',
				wording: "L'entreprise a-t-elle des enjeux d'identité et d'accès ?",
				options: [
					['forts', 'Forts', 1],
					['moyens', 'Moyens', 0.5],
					['faibles', 'Faibles', 0],
					['inconnu', 'Inconnus', 0.3, 'unknown'],
				],
			},
		],
	},
	{
		id: 'exclusions',
		label: 'Exclusions',
		description:
			'Écarte un lead pour une raison explicite : éditeur ou intégrateur, hébergeur ou agence, domaine inactif, client actuel, concurrent.',
		name: 'Exclusions',
		evaluates:
			"Les raisons d'écarter le lead : éditeur ou intégrateur du domaine, hébergeur ou agence, domaine parqué ou inactif, client actuel, concurrent.",
		prompt:
			"Tu décides si une entreprise doit être écartée des prospects. Tu n'évalues pas son intérêt : tu cherches uniquement une raison explicite de l'exclure. Choisis une option qui exclut seulement quand une phrase des documents le dit explicitement et copie cette phrase mot pour mot comme preuve. Un nom proche d'une entreprise de la liste, une simple ressemblance ou une impression ne suffisent pas : réponds alors « non » ou « inconnu ». Quand rien dans les documents ne permet de trancher, réponds « inconnu ».\n\nLes listes suivantes sont maintenues par l'opérateur. Modifie-les pour adapter l'agent à ton marché.\n\nClients actuels : (liste à compléter)\nConcurrents : Okta, Auth0, Ping Identity, ForgeRock, Microsoft Entra, Keycloak, Red Hat\nÉditeurs et intégrateurs du domaine : Thales, Wallix, Ilex, Inwebo, Ubble, IDnow, Evidian, Eviden, Atos, Capgemini, Sopra Steria, Wavestone\n\nPour « hébergeur ou agence », exclus seulement quand le signal détecté concerne un client de l'entreprise (elle héberge ou opère le site d'un tiers) et non l'entreprise elle-même. Pour « domaine parqué ou inactif », exclus seulement quand les documents montrent une page de parking, un site en vente ou un domaine sans activité.",
		weight: 1,
		axes: [
			{
				id: 'editeur_integrateur',
				wording:
					"L'entreprise est-elle un éditeur ou un intégrateur du domaine ?",
				options: [
					['oui', 'Éditeur ou intégrateur du domaine', 0, 'excludes'],
					['non', 'Non', 1],
					['inconnu', 'Inconnu', 0.5, 'unknown'],
				],
			},
			{
				id: 'hebergeur_agence',
				wording:
					"Le signal concerne-t-il un client de l'entreprise (hébergeur ou agence) plutôt que l'entreprise elle-même ?",
				options: [
					['oui', "Hébergeur ou agence (signal d'un client)", 0, 'excludes'],
					['non', 'Non', 1],
					['inconnu', 'Inconnu', 0.5, 'unknown'],
				],
			},
			{
				id: 'domaine_inactif',
				wording: 'Le domaine est-il parqué ou inactif ?',
				options: [
					['parque_ou_inactif', 'Domaine parqué ou inactif', 0, 'excludes'],
					['actif', 'Actif', 1],
					['inconnu', 'Inconnu', 0.5, 'unknown'],
				],
			},
			{
				id: 'client_actuel',
				wording: "L'entreprise est-elle déjà cliente ?",
				options: [
					['oui', 'Client actuel', 0, 'excludes'],
					['non', 'Non', 1],
					['inconnu', 'Inconnu', 0.5, 'unknown'],
				],
			},
			{
				id: 'concurrent',
				wording: "L'entreprise est-elle un concurrent ?",
				options: [
					['oui', 'Concurrent', 0, 'excludes'],
					['non', 'Non', 1],
					['inconnu', 'Inconnu', 0.5, 'unknown'],
				],
			},
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
		weight: template.weight ?? DEFAULT_WEIGHT,
		exclusion_threshold: DEFAULT_EXCLUSION_THRESHOLD,
		questions: template.axes.map((axis) => ({
			id: axis.id,
			wording: axis.wording,
			weight: axis.weight ?? DEFAULT_WEIGHT,
			options: axis.options.map(([value, label, score, flag]) => ({
				value,
				label,
				score,
				is_unknown: flag === 'unknown',
				excludes: flag === 'excludes',
			})),
		})),
	}
}
