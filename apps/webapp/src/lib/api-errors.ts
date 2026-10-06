import { z } from 'zod'

import { draftStatusLabel } from '#/pages/drafts/lib/labels'
import { leadStatusLabel } from '#/pages/leads/lib/labels'
import { LEAD_STATUSES } from '#/pages/leads/lib/transitions'

const errorBodySchema = z.object({
	message: z.string(),
	details: z
		.array(z.object({ path: z.string(), message: z.string() }))
		.optional(),
})

export interface ParsedApiError {
	message: string
	status: number | null
	fields: Record<string, string>
}

const EXACT_MESSAGES: Readonly<Record<string, string>> = {
	'must not be empty': 'Ne doit pas être vide',
	'at least one agent is required': 'Au moins un agent est requis',
	'at least one qualification agent is required':
		'Au moins un agent de qualification est requis',
	'must be lowercase letters, digits, hyphens and underscores, at most 63 characters':
		'Doit être composé de minuscules, chiffres, tirets et tirets bas, 63 caractères au maximum',
	'must have from 2 to 10 options': 'Doit avoir de 2 à 10 options',
	'must be a number from 0 to 1': 'Doit être un nombre de 0 à 1',
	'must be higher than `discard_below`':
		'Doit être supérieur au seuil « Écarté sous »',
	'must be a slug of lowercase letters, digits and hyphens, at most 63 characters':
		'Doit être un identifiant en minuscules, chiffres et tirets, 63 caractères au maximum',
	'invalid regular expression': 'Expression régulière invalide',
	'regular expression needs one capture group':
		"L'expression régulière doit contenir un groupe de capture",
	'must be an absolute path starting with `/`':
		'Doit être un chemin absolu commençant par `/`',
	'HTTP status must be between 100 and 599':
		'Le statut HTTP doit être compris entre 100 et 599',
	'must be a lowercase hostname label':
		"Doit être un libellé d'hôte en minuscules",
	'must be a lowercase domain suffix with at least one dot':
		'Doit être un suffixe de domaine en minuscules avec au moins un point',
	'must be the local part of an email address, without `@`':
		"Doit être la partie locale d'une adresse e-mail, sans `@`",
	'must also be listed in `preferred_local_parts`':
		'Doit aussi figurer dans les parties locales préférées',
	'must be an integer from 1 to 10': 'Doit être un entier de 1 à 10',
	'must be a percentage from 0 to 100': 'Doit être un pourcentage de 0 à 100',
	'must be a dotted numeric version':
		'Doit être une version numérique pointée (ex. 1.2.3)',
	'must be at least 1': 'Doit valoir au moins 1',
	'`version_below` needs a version extractor on the fingerprint':
		"`version_below` nécessite un extracteur de version sur l'empreinte",
	'must be a finite number': 'Doit être un nombre fini',
	'must be a string': 'Doit être une chaîne de caractères',
	'must be a string or null': 'Doit être une chaîne de caractères ou null',
	'must be an http or https URL': 'Doit être une URL http ou https',
	'must be a non-negative integer': 'Doit être un entier positif ou nul',
	'must be a non-negative number': 'Doit être un nombre positif ou nul',
	'must be a number': 'Doit être un nombre',
	'must be a number or null': 'Doit être un nombre ou null',
	'must be an object': 'Doit être un objet',
	'must be a JSON object': 'Doit être un objet JSON',
	'must be a UUID': 'Doit être un UUID',
	'must be a whole number': 'Doit être un nombre entier',
	'must be json or csv': 'Doit être json ou csv',
	'must be a lowercase snake_case signal kind':
		'Doit être un type de signal en snake_case minuscule',
	'must be `triage` or `qualification`':
		'Doit être `triage` ou `qualification`',
	'unknown field': 'Champ inconnu',
	'unknown provider': 'Fournisseur inconnu',
	'is required': 'Champ obligatoire',
	'is required to reject': 'Choisissez une raison de rejet',
	'is required when the reason is autre':
		'Précisez la raison quand elle est « Autre »',
	'is required for this provider kind':
		'Obligatoire pour ce type de fournisseur',
	'is too long': 'Trop long',
	'malformed JSON': 'JSON mal formé',
	'at least one of subject or body is required':
		"Au moins l'objet ou le corps est requis",
	'a provider with this name already exists':
		'Un fournisseur portant ce nom existe déjà',
	'a profile with this name already exists':
		'Un profil portant ce nom existe déjà',
	'source is already running': 'La source est déjà en cours',
	'source has no scheduled job': "La source n'a pas de tâche planifiée",
	'must be a lowercase source name': 'Doit être un nom de source en minuscules',
	'source is not scheduled': "La source n'est plus planifiée",
	'source listens continuously':
		"La source écoute en continu, il n'y a rien à lancer",
	'provider is assigned to a tier':
		'Le fournisseur est un fournisseur par défaut',
	'lead status changed concurrently, reload and retry':
		'Le statut du lead a changé entre-temps, rechargez la page et réessayez',
	'revision number overflow': 'Nombre de révisions maximal atteint',
	'Request payload rejected': 'Requête rejetée',
	'Authentication failed': "Échec de l'authentification",
	'Authorization header is missing': "En-tête d'autorisation manquant",
	'Invalid authorization header': "En-tête d'autorisation invalide",
	'Internal server error': 'Erreur interne du serveur',
	Unauthorized: 'Non autorisé',
	'Token not found': 'Jeton introuvable',
	'Token expired': 'Jeton expiré',
	'Invalid token': 'Jeton invalide',
	Forbidden: 'Accès interdit',
	'Resource not found': 'Ressource introuvable',
	'Too many requests': 'Trop de requêtes, réessayez dans un instant',
	'Database error': 'Erreur de base de données',
	'Unexpected error': 'Erreur inattendue',
	'Failed to fetch': 'Serveur injoignable',
	'Load failed': 'Serveur injoignable',
	'NetworkError when attempting to fetch resource.': 'Serveur injoignable',
	'The test failed': 'Le test a échoué',
	'The provider did not answer in time.':
		"Le fournisseur n'a pas répondu à temps.",
	'The provider rate limited the request.':
		'Le fournisseur a limité le débit de la requête.',
	'The provider rejected the credentials.':
		'Le fournisseur a refusé les identifiants.',
	'The endpoint could not be reached.': "Le point d'accès est injoignable.",
	'The provider answered with an unusable response.':
		'Le fournisseur a renvoyé une réponse inexploitable.',
	'The provider configuration is not usable.':
		'La configuration du fournisseur est inutilisable.',
	'The model used all its tokens to reason; lower its reasoning effort.':
		'Le modèle a utilisé tous ses tokens pour raisonner.',
	'The stored API key could not be decrypted; set it again.':
		'La clé API enregistrée est indéchiffrable, saisissez-la à nouveau.',
}

const HTTP_STATUS_MESSAGES: Readonly<Record<number, string>> = {
	400: 'Requête invalide',
	401: 'Authentification requise',
	403: 'Accès interdit',
	404: 'Ressource introuvable',
	409: "Conflit avec l'état actuel",
	422: 'Requête rejetée',
	429: 'Trop de requêtes, réessayez dans un instant',
	500: 'Erreur interne du serveur',
	502: 'Passerelle indisponible',
	503: 'Service indisponible',
	504: 'Délai de la passerelle dépassé',
}

function leadStatusName(raw: string): string {
	const status = LEAD_STATUSES.find((candidate) => candidate === raw)
	return status ? leadStatusLabel(status).toLowerCase() : raw
}

function draftStatusName(raw: string): string {
	return draftStatusLabel(raw.toLowerCase()).toLowerCase()
}

function lowerFirst(text: string): string {
	return text.charAt(0).toLowerCase() + text.slice(1)
}

function tierName(raw: string): string {
	if (raw === 'triage') return 'Classification'
	if (raw === 'qualification') return 'Synthèse'
	return raw
}

type Rule = readonly [RegExp, (match: RegExpMatchArray) => string]

const RULES: readonly Rule[] = [
	[/^malformed JSON: .*$/s, () => 'JSON mal formé'],
	[/^malformed: unknown field `(.*?)`.*$/s, (m) => `Champ inconnu \`${m[1]}\``],
	[
		/^malformed: missing field `(.*?)`.*$/s,
		(m) => `Champ manquant \`${m[1]}\``,
	],
	[/^malformed: (.*)$/s, (m) => `Format invalide : ${m[1]}`],
	[
		/^unsupported schema version (\d+), expected 1, 2, 3 or (\d+)$/,
		(m) =>
			`Version de schéma ${m[1]} non prise en charge, attendu : 1, 2, 3 ou ${m[2]}`,
	],
	[
		/^unsupported schema version (\d+), expected 1, 2 or (\d+)$/,
		(m) =>
			`Version de schéma ${m[1]} non prise en charge, attendu : 1, 2 ou ${m[2]}`,
	],
	[/^duplicate value `(.*)`$/, (m) => `Valeur en double : \`${m[1]}\``],
	[
		/^must be at most (\d+) characters$/,
		(m) => `Doit contenir au maximum ${m[1]} caractères`,
	],
	[
		/^must be between (-?\d+(?:\.\d+)?) and (-?\d+(?:\.\d+)?)$/,
		(m) => `Doit être compris entre ${m[1]} et ${m[2]}`,
	],
	[/^must be at least (\d+(?:\.\d+)?)$/, (m) => `Doit valoir au moins ${m[1]}`],
	[/^must be at most (\d+(?:\.\d+)?)$/, (m) => `Doit valoir au plus ${m[1]}`],
	[/^must be one of (.+)$/, (m) => `Doit être l'une des valeurs : ${m[1]}`],
	[
		/^lead cannot move from (\w+) to (\w+)$/,
		(m) =>
			`Le lead ne peut pas passer de « ${leadStatusName(m[1] ?? '')} » à « ${leadStatusName(m[2] ?? '')} »`,
	],
	[
		/^draft cannot move from (\w+) to (\w+)$/,
		(m) =>
			`Le brouillon ne peut pas passer de « ${draftStatusName(m[1] ?? '')} » à « ${draftStatusName(m[2] ?? '')} »`,
	],
	[
		/^provider is assigned to the (\w+) tier$/,
		(m) =>
			`Le fournisseur est le fournisseur par défaut « ${tierName(m[1] ?? '')} »`,
	],
	[
		/^(\d+) leads match, more than the (\d+) an export holds, narrow the filters$/,
		(m) =>
			`${m[1]} leads correspondent, plus que les ${m[2]} qu'un export peut contenir : affinez les filtres`,
	],
	[
		/^The provider answered with status (\d+)\.$/,
		(m) => `Le fournisseur a répondu avec le statut ${m[1]}.`,
	],
	[
		/^Insufficient permissions: required scope `(.*)`$/,
		(m) => `Permissions insuffisantes : portée requise \`${m[1]}\``,
	],
	[
		/^(.+) with id `(.*)` not found$/,
		(m) => `${m[1]} introuvable (id \`${m[2]}\`)`,
	],
	[
		/^External service error: (.*)$/s,
		(m) => `Erreur d'un service externe : ${m[1]}`,
	],
	[
		/^HTTP (\d{3})(?:: .*)?$/,
		(m) => HTTP_STATUS_MESSAGES[Number(m[1])] ?? `Erreur HTTP ${m[1]}`,
	],
]

const PREFIXES: readonly (readonly [string, string])[] = [
	['Bad request: ', 'Requête invalide : '],
	['Validation failed: ', 'Validation échouée : '],
	['Conflict: ', ''],
	['Unprocessable entity: ', ''],
]

const FIELD_REASON = /^([A-Za-z_$][\w.[\]$]*): (.+)$/s

export function translateApiMessage(message: string): string {
	const exact = EXACT_MESSAGES[message]
	if (exact !== undefined) return exact
	for (const [pattern, build] of RULES) {
		const match = message.match(pattern)
		if (match) return build(match)
	}
	for (const [english, french] of PREFIXES) {
		if (message.startsWith(english)) {
			const rest = translateApiMessage(message.slice(english.length))
			return french === '' ? rest : `${french}${lowerFirst(rest)}`
		}
	}
	const fieldReason = message.match(FIELD_REASON)
	if (fieldReason) {
		const reason = translateApiMessage(fieldReason[2] ?? '')
		if (reason !== fieldReason[2]) {
			return `${fieldReason[1]} : ${lowerFirst(reason)}`
		}
	}
	return message
}

function readStatus(error: Error): number | null {
	const status = (error as Error & { status?: unknown }).status
	return typeof status === 'number' ? status : null
}

export function parseApiError(error: unknown): ParsedApiError {
	if (!(error instanceof Error)) {
		return {
			message: translateApiMessage('Unexpected error'),
			status: null,
			fields: {},
		}
	}
	const status = readStatus(error)
	const body = errorBodySchema.safeParse(
		(error as Error & { data?: unknown }).data,
	)
	if (!body.success) {
		return { message: translateApiMessage(error.message), status, fields: {} }
	}
	const fields: Record<string, string> = {}
	for (const detail of body.data.details ?? []) {
		if (!(detail.path in fields)) {
			fields[detail.path] = translateApiMessage(detail.message)
		}
	}
	return { message: translateApiMessage(body.data.message), status, fields }
}
