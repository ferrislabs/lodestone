import { describe, expect, it } from 'vitest'

import { parseApiError, translateApiMessage } from './api-errors'

function apiError(status: number, data: unknown): Error {
	return Object.assign(new Error('Request payload rejected'), { status, data })
}

describe('parseApiError', () => {
	it('maps detail paths to field messages', () => {
		const parsed = parseApiError(
			apiError(422, {
				code: 'E_UNPROCESSABLE_ENTITY',
				message: 'Request payload rejected',
				status: 422,
				details: [
					{ path: 'subject', message: 'must be at most 200 characters' },
					{ path: 'body', message: 'must not be empty' },
				],
			}),
		)
		expect(parsed.fields).toEqual({
			subject: 'Doit contenir au maximum 200 caractères',
			body: 'Ne doit pas être vide',
		})
		expect(parsed.status).toBe(422)
		expect(parsed.message).toBe('Requête rejetée')
	})

	it('keeps the first message of a repeated path', () => {
		const parsed = parseApiError(
			apiError(422, {
				code: 'x',
				message: 'm',
				status: 422,
				details: [
					{ path: 'body', message: 'first' },
					{ path: 'body', message: 'second' },
				],
			}),
		)
		expect(parsed.fields).toEqual({ body: 'first' })
	})

	it('returns no fields for a conflict', () => {
		const parsed = parseApiError(
			apiError(409, {
				code: 'E_CONFLICT',
				message: 'Already decided',
				status: 409,
			}),
		)
		expect(parsed.fields).toEqual({})
		expect(parsed.message).toBe('Already decided')
		expect(parsed.status).toBe(409)
	})

	it('falls back to the error message when the body is not an error body', () => {
		const parsed = parseApiError(apiError(500, { oops: true }))
		expect(parsed.fields).toEqual({})
		expect(parsed.message).toBe('Requête rejetée')
	})

	it('handles non-error values', () => {
		expect(parseApiError('boom')).toEqual({
			message: 'Erreur inattendue',
			status: null,
			fields: {},
		})
	})
})

describe('translateApiMessage', () => {
	it.each([
		['must not be empty', 'Ne doit pas être vide'],
		['is required', 'Champ obligatoire'],
		['must be an integer from 1 to 10', 'Doit être un entier de 1 à 10'],
		[
			'must be a percentage from 0 to 100',
			'Doit être un pourcentage de 0 à 100',
		],
		['at least one agent is required', 'Au moins un agent est requis'],
		['invalid regular expression', 'Expression régulière invalide'],
		['Authentication failed', "Échec de l'authentification"],
		['Too many requests', 'Trop de requêtes, réessayez dans un instant'],
		[
			'a provider with this name already exists',
			'Un fournisseur portant ce nom existe déjà',
		],
		['Internal server error', 'Erreur interne du serveur'],
		['The test failed', 'Le test a échoué'],
	])('translates %s', (english, french) => {
		expect(translateApiMessage(english)).toBe(french)
	})

	it('translates parameterized messages', () => {
		expect(translateApiMessage('must be at most 3 characters')).toBe(
			'Doit contenir au maximum 3 caractères',
		)
		expect(translateApiMessage('must be between 1 and 100')).toBe(
			'Doit être compris entre 1 et 100',
		)
		expect(translateApiMessage('must be between -2 and 2')).toBe(
			'Doit être compris entre -2 et 2',
		)
		expect(translateApiMessage('must be a number or null')).toBe(
			'Doit être un nombre ou null',
		)
		expect(translateApiMessage('duplicate value `ct`')).toBe(
			'Valeur en double : `ct`',
		)
		expect(
			translateApiMessage('unsupported schema version 9, expected 1, 2 or 3'),
		).toBe('Version de schéma 9 non prise en charge, attendu : 1, 2 ou 3')
		expect(
			translateApiMessage(
				'unsupported schema version 9, expected 1, 2, 3 or 4',
			),
		).toBe('Version de schéma 9 non prise en charge, attendu : 1, 2, 3 ou 4')
		expect(
			translateApiMessage('at least one qualification agent is required'),
		).toBe('Au moins un agent de qualification est requis')
		expect(translateApiMessage('must have from 2 to 10 options')).toBe(
			'Doit avoir de 2 à 10 options',
		)
		expect(translateApiMessage('must be a number from 0 to 1')).toBe(
			'Doit être un nombre de 0 à 1',
		)
		expect(translateApiMessage('provider is assigned to the triage tier')).toBe(
			'Le fournisseur est le fournisseur par défaut « Classification »',
		)
		expect(translateApiMessage('The provider answered with status 502.')).toBe(
			'Le fournisseur a répondu avec le statut 502.',
		)
		expect(translateApiMessage('HTTP 503: Service Unavailable')).toBe(
			'Service indisponible',
		)
		expect(translateApiMessage('HTTP 418: Teapot')).toBe('Erreur HTTP 418')
	})

	it('translates state conflicts with french status names', () => {
		expect(
			translateApiMessage('Conflict: lead cannot move from new to validated'),
		).toBe('Le lead ne peut pas passer de « nouveau » à « validé »')
		expect(
			translateApiMessage(
				'Conflict: draft cannot move from Approved to Rejected',
			),
		).toBe('Le brouillon ne peut pas passer de « approuvé » à « rejeté »')
	})

	it('translates prefixed field messages', () => {
		expect(
			translateApiMessage(
				'Validation failed: page: must be between 1 and 1000000',
			),
		).toBe('Validation échouée : page : doit être compris entre 1 et 1000000')
		expect(translateApiMessage('Bad request: profile_id: must be a UUID')).toBe(
			'Requête invalide : profile_id : doit être un UUID',
		)
	})

	it('translates serde problems', () => {
		expect(
			translateApiMessage('malformed: unknown field `extra`, expected `offer`'),
		).toBe('Champ inconnu `extra`')
		expect(translateApiMessage('malformed: missing field `name`')).toBe(
			'Champ manquant `name`',
		)
	})

	it('keeps unknown messages as they are', () => {
		expect(translateApiMessage('something unforeseen')).toBe(
			'something unforeseen',
		)
	})
})

describe('source run errors', () => {
	it('translates the run-now messages', () => {
		expect(translateApiMessage('Conflict: source is already running')).toBe(
			'La source est déjà en cours',
		)
		expect(translateApiMessage('Conflict: source listens continuously')).toBe(
			"La source écoute en continu, il n'y a rien à lancer",
		)
		expect(translateApiMessage('source has no scheduled job')).toBe(
			"La source n'a pas de tâche planifiée",
		)
		expect(translateApiMessage('Source with id `nope` not found')).toBe(
			'Source introuvable (id `nope`)',
		)
	})

	it('keeps the conflict status for a running source', () => {
		const parsed = parseApiError(
			apiError(409, {
				code: 'E_CONFLICT',
				message: 'Conflict: source is already running',
				status: 409,
			}),
		)
		expect(parsed.status).toBe(409)
		expect(parsed.message).toBe('La source est déjà en cours')
	})
})

describe('rejection messages', () => {
	it('translates the reason requirements', () => {
		expect(translateApiMessage('is required to reject')).toBe(
			'Choisissez une raison de rejet',
		)
		expect(translateApiMessage('is required when the reason is autre')).toBe(
			'Précisez la raison quand elle est « Autre »',
		)
		expect(translateApiMessage('must be at most 500 characters')).toBe(
			'Doit contenir au maximum 500 caractères',
		)
	})
})
