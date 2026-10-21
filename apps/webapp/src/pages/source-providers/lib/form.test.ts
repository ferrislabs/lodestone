import { describe, expect, it } from 'vitest'

import { detail } from '#/pages/source-providers/lib/fixtures'
import {
	buildBody,
	fieldErrors,
	initialValues,
	isDirty,
} from '#/pages/source-providers/lib/form'

describe('provider form', () => {
	it('starts from the stored settings and never from a secret', () => {
		expect(initialValues(detail())).toEqual({
			client_id: 'client-1',
			api_base_url: 'https://api.example.com/v2',
		})
	})

	it('falls back to defaults for an unconfigured provider', () => {
		const empty = detail({ settings: {}, secrets: [] })
		expect(initialValues(empty)).toEqual({
			client_id: '',
			api_base_url: 'https://api.example.com/v2',
		})
	})

	it('builds a body without untouched secrets', () => {
		const current = detail()
		expect(
			buildBody(current, { client_id: ' client-2 ', api_base_url: '' }, {}),
		).toEqual({ settings: { client_id: 'client-2' }, secrets: {} })
	})

	it('sends only the secrets the operator typed', () => {
		expect(
			buildBody(
				detail(),
				{ client_id: 'client-1', api_base_url: 'https://x.test' },
				{ client_secret: 'new-secret' },
			).secrets,
		).toEqual({ client_secret: 'new-secret' })
		expect(
			buildBody(
				detail(),
				{ client_id: 'client-1', api_base_url: 'https://x.test' },
				{ client_secret: '   ' },
			).secrets,
		).toEqual({})
	})

	it('converts numbers and keeps booleans', () => {
		const typed = detail({
			fields: [
				{
					key: 'limit',
					label: 'Limite',
					kind: 'number',
					required: false,
					default: null,
					help: '',
					options: [],
				},
				{
					key: 'remote',
					label: 'Télétravail',
					kind: 'bool',
					required: false,
					default: 'false',
					help: '',
					options: [],
				},
			],
			settings: {},
			secrets: [],
		})
		expect(initialValues(typed)).toEqual({ limit: '', remote: false })
		expect(buildBody(typed, { limit: '12,5', remote: true }, {})).toEqual({
			settings: { limit: 12.5, remote: true },
			secrets: {},
		})
	})

	it('tracks unsaved changes', () => {
		const current = detail()
		const values = initialValues(current)
		expect(isDirty(current, values, {})).toBe(false)
		expect(isDirty(current, { ...values, client_id: 'other' }, {})).toBe(true)
		expect(isDirty(current, values, { client_secret: 'x' })).toBe(true)
	})

	it('maps server paths onto field keys', () => {
		expect(
			fieldErrors({
				client_id: 'Champ obligatoire',
				'secrets.client_secret': 'Doit être une chaîne de caractères',
			}),
		).toEqual({
			client_id: 'Champ obligatoire',
			client_secret: 'Doit être une chaîne de caractères',
		})
	})
})
