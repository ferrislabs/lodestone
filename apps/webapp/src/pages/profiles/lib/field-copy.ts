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

export const LEARNED_CALIBRATION_COPY: FieldCopy = {
	label: 'Calibration apprise',
	description:
		"Corrige chaque jour les probabilités de cet agent d'après vos décisions, dès 50 leads étiquetés dont au moins 10 de chaque classe.",
}

const FINGERPRINT = 'enrichment.custom.fingerprints[]'
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
		description:
			'Agents de sourcing : ils trouvent les leads. Au moins un est requis, sauf si le profil a ses propres empreintes ou des mots-clés à mettre en avant.',
	},
	'agents[]': {
		label: 'Agent',
		description:
			'Un agent de recherche interroge des fournisseurs du catalogue, un agent Certificats lit les journaux de certificats, un agent DNS reconnaît des fournisseurs par la cible CNAME.',
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
	'agents[].labels': {
		label: 'Libellés',
		description:
			"Libellés de nom d'hôte à rechercher, par exemple auth ou sso.",
	},
	'agents[].countries': {
		label: 'Pays',
		description:
			"Pays visés. Un hôte sous l'extension d'un autre pays ne crée pas de lead ; une extension générique en crée un, retiré du profil si le siège de l'entreprise est ailleurs.",
	},
	'agents[].patterns': {
		label: 'Motifs CNAME',
		description: 'Couples fournisseur et suffixe de cible.',
	},
	'agents[].patterns[]': {
		label: 'Motif CNAME',
		description: "Un fournisseur et le suffixe de cible CNAME qui l'identifie.",
		add: 'Ajouter un motif',
	},
	'agents[].patterns[].provider': {
		label: 'Fournisseur',
		description: 'Nom du fournisseur.',
	},
	'agents[].patterns[].target_suffix': {
		label: 'Suffixe cible',
		description: 'Suffixe de la cible CNAME qui identifie le fournisseur.',
	},
	inspection: {
		label: "Inspection de l'écosystème",
		description:
			'Chaque lead avec un domaine reçoit la même inspection, quel que soit le profil : identité officielle, vie légale, marchés publics, site et stack technique, DNS et e-mail, recrutement et activité récente.',
	},
	'inspection.enabled': {
		label: "Inspecter l'écosystème des leads",
		description:
			"Désactivée, les leads du profil ne sont pas inspectés et la qualification n'attend pas l'inspection.",
	},
	enrichment: {
		label: 'Enrichissement',
		description:
			"Réglages de l'inspection propres à ce profil. Tout le reste est identique pour chaque profil.",
	},
	'enrichment.custom': {
		label: 'Empreintes propres au profil',
		description:
			'Empreintes sondées sur le site de chaque lead du profil, produits ignorés et sous-domaines sondés en plus.',
	},
	'enrichment.custom.extra_subdomains': {
		label: 'Sous-domaines supplémentaires',
		description:
			'Sous-domaines du domaine du lead sondés en plus avec vos empreintes, par exemple auth ou sso. Vide par défaut.',
	},
	'enrichment.custom.fingerprints': {
		label: 'Empreintes',
		description: 'Empreintes à sonder sur le site de chaque lead.',
	},
	'enrichment.custom.excluded_products': {
		label: 'Produits exclus',
		description: 'Produits à ignorer, par exemple les vôtres.',
	},
	'enrichment.highlight_keywords': {
		label: 'Mots-clés à mettre en avant',
		description:
			"Mots-clés cherchés dans les offres d'emploi des leads. Une offre qui les contient est mise en avant et devient un signal. Vide : les mots-clés des agents de recherche activés.",
	},
	[FINGERPRINT]: {
		label: 'Empreinte',
		description:
			"Empreinte d'un produit : chemin à requêter, conditions, extracteur de version facultatif et marqueurs de dette.",
		add: 'Ajouter une empreinte',
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
	'agents[].filters': {
		label: 'Filtres',
		description:
			"Critères communs à tous les fournisseurs de l'agent. Chaque fournisseur filtre ce que son API permet, le reste est appliqué après la collecte.",
	},
	'agents[].filters.remote': {
		label: 'Télétravail',
		description:
			"Indifférent, full remote uniquement, ou remote et hybride. Ne concerne que les offres d'emploi.",
	},
	'agents[].filters.contracts': {
		label: 'Contrat',
		description:
			'Types de contrat acceptés. Sans choix, ou avec « Tous les contrats », aucun filtre.',
	},
	'agents[].providers': {
		label: 'Fournisseurs',
		description:
			'Fournisseurs du catalogue interrogés par cet agent. Au moins un est requis.',
	},
	'agents[].filters.countries': {
		label: 'Pays',
		description:
			'Codes pays ISO à deux lettres en majuscules, par exemple FR. Vide : tous les pays.',
	},
	'agents[].keywords': {
		label: 'Mots-clés',
		description:
			'Une annonce est gardée si elle contient au moins un de ces mots-clés, en mot entier.',
	},
	'agents[].accept_min_keywords': {
		label: 'Mots-clés distincts pour un signal fort',
		description:
			"Nombre de mots-clés distincts qu'une annonce doit contenir pour donner un signal fort. En dessous, l'annonce est gardée avec un signal plus faible. Une annonce sans aucun mot-clé est écartée.",
	},
	'agents[].exclusions': {
		label: 'Exclusions',
		description:
			"Mots qui écartent une annonce lorsqu'ils y figurent, même si elle contient assez de mots-clés.",
	},
	'qualification_agents[].learned_calibration': LEARNED_CALIBRATION_COPY,
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
	'qualification_agents[].exclusion_threshold': {
		label: "Seuil d'exclusion",
		description:
			"Probabilité minimale, de 0,5 à 1, d'une option qui exclut le lead pour l'écarter. 0,6 par défaut.",
	},
	'qualification_agents[].samples': {
		label: 'Échantillons',
		description:
			"Nombre d'appels indépendants au modèle par lead, de 1 à 5. La réponse de chaque axe est le choix majoritaire. 3 par défaut.",
	},
	'qualification_agents[].sampling_temperature': {
		label: "Température d'échantillonnage",
		description:
			"Température de 0 à 1,5 des appels quand il y a plus d'un échantillon ; elle remplace celle du fournisseur. 0,7 par défaut.",
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
	'qualification_agents[].questions[].options[].excludes': {
		label: 'Exclut le lead',
		description:
			"Quand l'agent choisit cette option avec une preuve vérifiée et une probabilité au moins égale au seuil d'exclusion, le lead est écarté. Jamais sur l'option « inconnu ».",
	},
	'scoring_agents[].learned_calibration': LEARNED_CALIBRATION_COPY,
	scoring_agents: {
		label: 'Agents de scoring',
		description:
			'Chaque agent décide du score et du verdict de chaque lead, avec une réponse de forme imposée. Aucun agent actif : le verdict reste en attente.',
	},
	'scoring_agents[]': {
		label: 'Agent de scoring',
		description:
			'Répond à des critères à choix, donne un score de 0 à 100, un verdict avec ses probabilités et ses raisons.',
	},
	'scoring_agents[].id': {
		label: 'Identifiant',
		description:
			'Identifiant unique parmi tous les agents du profil, par exemple score-global.',
	},
	'scoring_agents[].name': {
		label: 'Nom',
		description: "Nom d'affichage.",
	},
	'scoring_agents[].description': {
		label: 'Ce que cet agent évalue',
		description:
			"Ce que l'agent évalue, en une phrase. Affiché sur sa carte et dans la fiche du lead.",
	},
	'scoring_agents[].enabled': {
		label: 'Activé',
		description: "Un agent désactivé est conservé mais n'est pas exécuté.",
	},
	'scoring_agents[].provider_id': {
		label: 'Fournisseur LLM',
		description:
			"Fournisseur LLM déclaré qui répond pour cet agent. Le fournisseur Synthèse par défaut s'applique à défaut.",
	},
	'scoring_agents[].weight': {
		label: "Poids de l'agent",
		description:
			'Poids entier de 1 à 10 de cet agent dans la moyenne pondérée du score et du verdict du lead. Affiché à partir de deux agents de scoring.',
	},
	'scoring_agents[].system_prompt': {
		label: 'Prompt système',
		description: 'Consigne donnée au modèle, 8000 caractères au maximum.',
	},
	'scoring_agents[].criteria': {
		label: 'Critères',
		description:
			"Critères auxquels l'agent répond avant de donner son score. Au moins un est requis.",
	},
	'scoring_agents[].criteria[]': {
		label: 'Critère',
		description: 'Une question à choix posée au modèle à propos du lead.',
		add: 'Ajouter un critère',
	},
	'scoring_agents[].criteria[].id': {
		label: 'Identifiant du critère',
		description: "Identifiant unique dans l'agent, clé de la réponse.",
	},
	'scoring_agents[].criteria[].wording': {
		label: 'Intitulé du critère',
		description: 'La question posée au modèle.',
	},
	'scoring_agents[].criteria[].weight': {
		label: 'Poids du critère',
		description: 'Poids entier de 1 à 10 de ce critère.',
	},
	'scoring_agents[].criteria[].options': {
		label: 'Options',
		description:
			'Réponses possibles, de 2 à 11, de valeurs uniques, dont exactement une option « inconnu ».',
	},
	'scoring_agents[].criteria[].options[]': {
		label: 'Option',
		description: "Une réponse possible et le score qu'elle vaut.",
		add: 'Ajouter une option',
	},
	'scoring_agents[].criteria[].options[].value': {
		label: 'Valeur',
		description: 'Valeur renvoyée par le modèle, unique dans le critère.',
	},
	'scoring_agents[].criteria[].options[].label': {
		label: 'Libellé',
		description: "Libellé d'affichage de l'option.",
	},
	'scoring_agents[].criteria[].options[].score': {
		label: 'Score',
		description: 'Score de 0 à 1 de cette réponse.',
	},
	'scoring_agents[].criteria[].options[].is_unknown': {
		label: 'Option « inconnu »',
		description:
			"Marque l'option choisie quand rien dans les documents n'étaye une réponse. Chaque critère en a exactement une.",
	},
	'scoring_agents[].criteria[].options[].excludes': {
		label: 'Exclut le lead',
		description:
			'Inutilisé : seuls les agents de qualification peuvent exclure un lead.',
	},
	'scoring_agents[].samples': {
		label: 'Échantillons',
		description:
			"Nombre d'appels indépendants au modèle par lead, de 1 à 5. Les critères sont votés, le score et les probabilités du verdict sont moyennés. 1 par défaut.",
	},
	'scoring_agents[].sampling_temperature': {
		label: "Température d'échantillonnage",
		description:
			"Température de 0 à 1,5 des appels quand il y a plus d'un échantillon ; elle remplace celle du fournisseur. 0,7 par défaut.",
	},
	escalation: {
		label: 'Escalade',
		description:
			'Réévalue par un fournisseur plus puissant les leads dont la confiance tombe dans une bande grise.',
	},
	'escalation.provider': {
		label: "Fournisseur d'escalade",
		description:
			"Fournisseur LLM déclaré qui réévalue les leads de la bande grise. L'escalade est désactivée quand il est vide.",
	},
	'escalation.band_low': {
		label: 'À partir de',
		description:
			'Confiance minimale, en pourcentage, à partir de laquelle un lead est escaladé. 40 par défaut.',
	},
	'escalation.band_high': {
		label: "Jusqu'à (exclu)",
		description:
			'Confiance, en pourcentage, sous laquelle un lead est escaladé. Doit être supérieure à la borne basse. 70 par défaut.',
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
			"Solidité des preuves derrière un lead (signaux indépendants et vieillis, diversité des sources et des types, environnement des hôtes, force des signaux), calculée sur l'entreprise entière quand plusieurs domaines partagent un SIREN. Elle pondère l'avis des agents dans la confiance.",
	},
	'evidence.influence': {
		label: 'Influence des preuves',
		description:
			"Confiance = avis des agents × ((1 − influence) + influence × solidité des preuves). 0 % ignore les preuves, 100 % multiplie l'avis par elles.",
	},
	'evidence.weights': {
		label: 'Poids des composantes',
		description:
			'Poids relatif de chaque composante dans la solidité des preuves. Un poids à 0 ignore la composante, au moins un doit être supérieur à 0.',
	},
	'evidence.weights.volume': {
		label: 'Volume de signaux',
		description:
			'Poids du nombre de signaux indépendants, chacun diminué selon son âge, de 0 à 10.',
	},
	'evidence.weights.diversity': {
		label: 'Diversité des sources',
		description:
			'Poids du nombre de sources et de types de signaux distincts, de 0 à 10.',
	},
	'evidence.weights.environment': {
		label: 'Environnement',
		description:
			'Poids de la part des hôtes de production parmi les hôtes portant des signaux, de 0 à 10.',
	},
	'evidence.weights.strength': {
		label: 'Force des signaux',
		description:
			'Poids de la force des signaux : moyenne des trois signaux indépendants les plus forts, chacun avec la force donnée par sa source et diminué selon son âge, de 0 à 10.',
	},
	'evidence.half_life_days': {
		label: 'Demi-vie des signaux (jours)',
		description:
			"Âge en jours auquel un signal ne compte plus que pour moitié dans le volume et la force des preuves, par type de signal. « default » s'applique aux types non listés.",
	},
	contact_ranking: {
		label: 'Contacts',
		description:
			'Quelles adresses e-mail retenir pour contacter une entreprise détectée.',
	},
	'contact_ranking.preferred_local_parts': {
		label: 'Adresses à privilégier',
		description:
			"Débuts d'adresse e-mail (la partie avant le @, par exemple security ou contact) à retenir en priorité parmi celles trouvées, la plus recherchée en premier.",
	},
	'contact_ranking.guess_local_parts': {
		label: 'Adresses à deviner',
		description:
			"Débuts d'adresse e-mail à essayer lorsqu'aucune adresse n'est trouvée. Chacun doit aussi figurer dans les adresses à privilégier.",
	},
	language: {
		label: 'Langue des réponses',
		description:
			"Langue du bilan, de l'angle d'approche, du brouillon et des raisons de tri. Les citations restent dans la langue de la source.",
	},
}

export const VARIANT_TITLES: Readonly<Record<string, string>> = {
	search: 'Recherche',
	certificates: 'Certificats',
	dns: 'DNS',
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
