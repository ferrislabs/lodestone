import type { FieldCopy, UiMessages } from '#/lib/schema-form'

const MATCHER_NAME: FieldCopy = {
	label: "Nom de l'en-tête",
	description: "Nom de l'en-tête HTTP à examiner.",
}

const MATCHER_PATTERN: FieldCopy = {
	label: 'Expression régulière',
	description:
		"Expression régulière appliquée à la valeur de l'en-tête ou au corps de la réponse.",
}

const MATCHER_EQUALS: FieldCopy = {
	label: 'Code de statut attendu',
	description: 'Code de statut HTTP attendu dans la réponse.',
}

const FINGERPRINT = 'agents[].source.fingerprints[]'
const MARKER = `${FINGERPRINT}.debt_markers[]`

export const FIELD_COPY: Readonly<Record<string, FieldCopy>> = {
	identity: {
		label: 'Identité',
		description: "L'offre et le client cible, en texte libre.",
	},
	'identity.offer': {
		label: 'Offre',
		description: 'Ce qui est vendu, en texte libre.',
	},
	'identity.target_customer': {
		label: 'Client cible',
		description: 'Qui achète, en texte libre.',
	},
	agents: {
		label: 'Agents',
		description: 'Agents de détection. Au moins un est requis.',
	},
	'agents[]': {
		label: 'Agent',
		description: 'Recherche une source avec ses propres réglages.',
	},
	'agents[].id': {
		label: 'Identifiant',
		description: 'Identifiant unique dans le profil, par exemple ats-fr.',
	},
	'agents[].name': {
		label: 'Nom',
		description: "Nom d'affichage.",
	},
	'agents[].enabled': {
		label: 'Activé',
		description: "Un agent désactivé est conservé mais n'est pas exécuté.",
	},
	'agents[].source': {
		label: 'Source',
		description: 'La source interrogée et ses réglages.',
	},
	'agents[].llm': {
		label: "Réglages LLM de l'agent",
		description: 'Réglages LLM propres à cet agent.',
	},
	'agents[].llm.relevance_criteria': {
		label: 'Critères de pertinence',
		description:
			"Critères donnés au LLM pour trier les candidats de cet agent. Les critères du profil s'appliquent à défaut.",
	},
	'agents[].source.labels': {
		label: 'Libellés',
		description:
			"Libellés de nom d'hôte à rechercher, par exemple auth ou sso.",
	},
	'agents[].source.patterns': {
		label: 'Motifs CNAME',
		description: 'Couples fournisseur et suffixe de cible.',
	},
	'agents[].source.patterns[]': {
		label: 'Motif CNAME',
		description: "Un fournisseur et le suffixe de cible CNAME qui l'identifie.",
		add: 'Ajouter un motif',
	},
	'agents[].source.patterns[].provider': {
		label: 'Fournisseur',
		description: 'Nom du fournisseur.',
	},
	'agents[].source.patterns[].target_suffix': {
		label: 'Suffixe cible',
		description: 'Suffixe de la cible CNAME qui identifie le fournisseur.',
	},
	'agents[].source.fingerprints': {
		label: 'Empreintes',
		description: 'Empreintes à sonder sur chaque domaine.',
	},
	[FINGERPRINT]: {
		label: 'Empreinte',
		description:
			"Empreinte d'un produit : chemin à requêter, conditions, extracteur de version facultatif et marqueurs de dette.",
	},
	[`${FINGERPRINT}.id`]: {
		label: 'Identifiant',
		description: 'Identifiant unique.',
	},
	[`${FINGERPRINT}.path`]: {
		label: 'Chemin',
		description: "Chemin d'URL à requêter, par exemple /login.",
	},
	[`${FINGERPRINT}.product`]: {
		label: 'Produit',
		description: 'Nom du produit remonté comme signal.',
	},
	[`${FINGERPRINT}.matchers`]: {
		label: 'Conditions',
		description: 'Conditions sur la réponse. Toutes doivent être vraies.',
	},
	[`${FINGERPRINT}.matchers[]`]: {
		label: 'Condition',
		description: 'Une condition sur la réponse HTTP.',
		add: 'Ajouter une condition',
	},
	[`${FINGERPRINT}.matchers[].equals`]: MATCHER_EQUALS,
	[`${FINGERPRINT}.matchers[].name`]: MATCHER_NAME,
	[`${FINGERPRINT}.matchers[].pattern`]: MATCHER_PATTERN,
	[`${FINGERPRINT}.debt_markers`]: {
		label: 'Marqueurs de dette technique',
		description: 'Signaux de dette technique pour ce produit.',
	},
	[MARKER]: {
		label: 'Marqueur de dette',
		description: 'Signal de dette technique levé lorsque sa règle est vraie.',
	},
	[`${MARKER}.id`]: {
		label: 'Identifiant',
		description: 'Identifiant unique.',
	},
	[`${MARKER}.rule`]: {
		label: 'Règle',
		description: 'Condition qui lève le marqueur.',
	},
	[`${MARKER}.rule.matcher`]: {
		label: 'Condition',
		description: 'Condition sur la réponse qui doit être vraie.',
	},
	[`${MARKER}.rule.matcher.equals`]: MATCHER_EQUALS,
	[`${MARKER}.rule.matcher.name`]: MATCHER_NAME,
	[`${MARKER}.rule.matcher.pattern`]: MATCHER_PATTERN,
	[`${MARKER}.rule.version`]: {
		label: 'Version',
		description: 'Levé lorsque la version extraite est inférieure à celle-ci.',
	},
	[`${FINGERPRINT}.version`]: {
		label: 'Extracteur de version',
		description: 'Extracteur facultatif de la version du produit.',
	},
	[`${FINGERPRINT}.version.name`]: {
		label: "Nom de l'en-tête",
		description: "Nom de l'en-tête HTTP.",
	},
	[`${FINGERPRINT}.version.pattern`]: {
		label: 'Expression régulière',
		description:
			'Expression régulière avec un groupe de capture pour la version.',
	},
	'agents[].source.excluded_products': {
		label: 'Produits exclus',
		description: 'Produits à ignorer, par exemple les vôtres.',
	},
	'agents[].source.keywords': {
		label: 'Mots-clés',
		description: 'Mots-clés à rechercher.',
	},
	'agents[].source.accept_min_keywords': {
		label: 'Mots-clés distincts pour accepter sans LLM',
		description:
			"Nombre de mots-clés distincts requis pour accepter un candidat sans le LLM. En dessous, ou en présence d'une exclusion, il passe au tri par le LLM.",
	},
	'agents[].source.exclusions': {
		label: 'Exclusions',
		description:
			"Mots-clés qui envoient un candidat au tri par le LLM lorsqu'ils sont présents.",
	},
	qualification_agents: {
		label: 'Agents de qualification',
		description:
			'Chaque agent évalue tous les leads du profil sous un angle. Au moins un est requis.',
	},
	'qualification_agents[]': {
		label: 'Agent de qualification',
		description:
			"Mesure une liste d'axes d'analyse sur chaque lead et en rédige un résumé.",
	},
	'qualification_agents[].id': {
		label: 'Identifiant',
		description:
			'Identifiant unique parmi tous les agents du profil, par exemple maturite-technique.',
	},
	'qualification_agents[].name': {
		label: 'Nom',
		description: "Nom d'affichage.",
	},
	'qualification_agents[].description': {
		label: 'Ce que cet agent évalue',
		description:
			"Ce que l'agent évalue, en une phrase. Affiché sur sa carte et dans la fiche du lead.",
	},
	'qualification_agents[].enabled': {
		label: 'Activé',
		description: "Un agent désactivé est conservé mais n'est pas exécuté.",
	},
	'qualification_agents[].provider_id': {
		label: 'Fournisseur LLM',
		description:
			"Fournisseur LLM déclaré qui répond pour cet agent. Le fournisseur par défaut s'applique à défaut.",
	},
	'qualification_agents[].system_prompt': {
		label: 'Prompt système',
		description: 'Consigne donnée au modèle, 8000 caractères au maximum.',
	},
	'qualification_agents[].weight': {
		label: "Poids de l'agent",
		description:
			'Poids entier de 1 à 10 de cet agent dans la confiance du lead.',
	},
	'qualification_agents[].questions': {
		label: "Axes d'analyse",
		description:
			"Axes d'analyse que l'agent mesure sur chaque lead. Au moins un est requis.",
	},
	'qualification_agents[].questions[]': {
		label: "Axe d'analyse",
		description: 'Une question à choix posée au modèle à propos du lead.',
		add: "Ajouter un axe d'analyse",
	},
	'qualification_agents[].questions[].id': {
		label: "Identifiant de l'axe",
		description: "Identifiant unique dans l'agent, clé de la réponse.",
	},
	'qualification_agents[].questions[].wording': {
		label: "Intitulé de l'axe",
		description: 'La question posée au modèle.',
	},
	'qualification_agents[].questions[].weight': {
		label: "Poids de l'axe",
		description: "Poids entier de 1 à 10 de cet axe dans le score de l'agent.",
	},
	'qualification_agents[].questions[].options': {
		label: 'Options',
		description:
			'Réponses possibles, de 2 à 11, de valeurs uniques, dont exactement une option « inconnu ».',
	},
	'qualification_agents[].questions[].options[]': {
		label: 'Option',
		description: "Une réponse possible et le score qu'elle vaut.",
		add: 'Ajouter une option',
	},
	'qualification_agents[].questions[].options[].value': {
		label: 'Valeur',
		description: "Valeur renvoyée par le modèle, unique dans l'axe.",
	},
	'qualification_agents[].questions[].options[].label': {
		label: 'Libellé',
		description: "Libellé d'affichage de l'option.",
	},
	'qualification_agents[].questions[].options[].score': {
		label: 'Score',
		description: 'Score de 0 à 1 de cette réponse.',
	},
	'qualification_agents[].questions[].options[].is_unknown': {
		label: 'Option « inconnu »',
		description:
			"Marque l'option choisie quand rien dans les documents n'étaye une réponse. Chaque axe en a exactement une.",
	},
	aggregator: {
		label: 'Agrégateur',
		description:
			'Rédige le bilan du lead à partir des réponses de tous les agents de qualification.',
	},
	'aggregator.provider_id': {
		label: 'Fournisseur LLM',
		description:
			"Fournisseur LLM déclaré qui rédige le bilan. Le fournisseur par défaut s'applique à défaut.",
	},
	'aggregator.system_prompt': {
		label: 'Prompt système',
		description: 'Consigne donnée au modèle, 8000 caractères au maximum.',
	},
	verdict: {
		label: 'Verdicts et preuves',
		description:
			'Seuils de la confiance du lead qui décident de son verdict : fiable, à vérifier ou écarté, et conditions pour être fiable.',
	},
	'verdict.reliable_from': {
		label: 'Fiable à partir de',
		description: 'Un lead est fiable à partir de ce pourcentage de confiance.',
	},
	'verdict.discard_below': {
		label: 'Écarté sous',
		description:
			'Un lead est écarté sous ce pourcentage de confiance. Doit être inférieur au seuil fiable.',
	},
	'verdict.calibration_floor': {
		label: 'Plancher de calibration',
		description:
			"Chaque probabilité d'une réponse est ramenée entre ce plancher et son complément à 1, puis renormalisée avant le calcul. Un petit modèle ne peut plus affirmer une certitude absolue. 0 désactive la calibration, 0,2 au maximum.",
	},
	'verdict.min_evidence': {
		label: 'Preuves minimales pour être fiable',
		description:
			'Un lead ne peut être fiable que si la solidité de ses preuves atteint ce niveau. En dessous, il est plafonné à « À vérifier ».',
	},
	evidence: {
		label: 'Preuves',
		description:
			"Solidité des preuves derrière un lead (nombre de signaux, diversité des sources, environnement des hôtes, force des signaux). Elle pondère l'avis des agents dans la confiance.",
	},
	'evidence.influence': {
		label: 'Influence des preuves',
		description:
			"Confiance = avis des agents × ((1 − influence) + influence × solidité des preuves). 0 % ignore les preuves, 100 % multiplie l'avis par elles.",
	},
	'evidence.reference_score': {
		label: 'Score de référence',
		description:
			'Score des signaux à partir duquel la force des signaux vaut 100 %. Au moins 1.',
	},
	'evidence.weights': {
		label: 'Poids des composantes',
		description:
			'Poids relatif de chaque composante dans la solidité des preuves. Un poids à 0 ignore la composante, au moins un doit être supérieur à 0.',
	},
	'evidence.weights.volume': {
		label: 'Volume de signaux',
		description: 'Poids du nombre de signaux détectés, de 0 à 10.',
	},
	'evidence.weights.diversity': {
		label: 'Diversité des sources',
		description: 'Poids du nombre de sources distinctes, de 0 à 10.',
	},
	'evidence.weights.environment': {
		label: 'Environnement',
		description:
			'Poids de la part des hôtes de production parmi les hôtes portant des signaux, de 0 à 10.',
	},
	'evidence.weights.strength': {
		label: 'Force des signaux',
		description: 'Poids du score des signaux du lead, de 0 à 10.',
	},
	scoring: {
		label: 'Scoring',
		description:
			"Facteurs pondérés. Le score d'un lead est la somme du poids multiplié par la mesure de chaque facteur.",
	},
	'scoring.factors': {
		label: 'Facteurs',
		description: 'Facteurs pondérés.',
	},
	'scoring.factors[]': {
		label: 'Facteur',
		description: 'Un type de signal pondéré.',
	},
	'scoring.factors[].signal': {
		label: 'Signal',
		description:
			"Type de signal. Le type réservé distinct_signals mesure le nombre de types de signaux distincts d'un lead.",
	},
	'scoring.factors[].value': {
		label: 'Valeur',
		description: 'Valeur du signal à retenir. Toute valeur si absente.',
	},
	'scoring.factors[].weight': {
		label: 'Poids',
		description: 'Poids entier de 1 à 10 appliqué à la mesure du signal.',
	},
	contact_ranking: {
		label: 'Classement des contacts',
		description:
			"Parties locales d'adresses e-mail préférées, la plus préférée en premier.",
	},
	'contact_ranking.preferred_local_parts': {
		label: 'Parties locales préférées',
		description:
			"Parties locales d'adresses e-mail préférées, la plus préférée en premier.",
	},
	'contact_ranking.guess_local_parts': {
		label: 'Parties locales à deviner',
		description:
			"Parties locales à deviner lorsqu'aucune adresse n'est trouvée.",
	},
	llm: {
		label: 'LLM',
		description:
			'Seuil de qualification, critères de pertinence et langue des réponses donnés au modèle.',
	},
	'llm.qualification_threshold': {
		label: 'Seuil de qualification',
		description:
			'Pourcentage du score maximal possible à partir duquel un lead est qualifié.',
	},
	'llm.relevance_criteria': {
		label: 'Critères de pertinence',
		description: "Critères donnés au LLM pour juger la pertinence d'un lead.",
	},
	'llm.language': {
		label: 'Langue des réponses du LLM',
		description:
			"Langue du bilan, de l'angle d'approche, du brouillon et des raisons de tri. Les citations restent dans la langue de la source.",
	},
}

export const VARIANT_TITLES: Readonly<Record<string, string>> = {
	certificate_transparency: 'Transparence des certificats',
	dns_cname: 'DNS CNAME',
	http_probe: 'Sonde HTTP',
	boamp: 'BOAMP',
	decp: 'DECP',
	ted: 'TED',
	france_travail: 'France Travail',
	ats: 'ATS',
	matches: 'Correspondance',
	version_below: 'Version inférieure à',
	status: 'Statut',
	header: 'En-tête',
	body: 'Corps',
}

export const FORM_MESSAGES: UiMessages = {
	required: 'Obligatoire',
	add: 'Ajouter',
	remove: 'Supprimer',
	addAria: (label) => `Ajouter : ${label}`,
	removeAria: (label, position) => `Supprimer : ${label} ${position}`,
	enableAria: (label) => `Activer : ${label}`,
	typeAria: (label) => `Type : ${label}`,
}
