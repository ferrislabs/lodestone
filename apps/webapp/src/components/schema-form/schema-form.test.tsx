import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { SchemaForm } from '#/components/schema-form/schema-form'
import {
	defaultValue,
	type JsonValue,
	type Problem,
	setIn,
} from '#/lib/schema-form'
import { loadProfileSchema } from '#/lib/schema-form/test-schema'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'

afterEach(cleanup)

const root = loadProfileSchema()

interface HarnessProps {
	initial?: JsonValue
	problems?: readonly Problem[]
	onValue?: (value: JsonValue) => void
}

function Harness({ initial, problems = [], onValue }: HarnessProps) {
	const [value, setValue] = useState<JsonValue>(
		initial ?? defaultValue(root, root, PROFILE_HINTS),
	)
	return (
		<SchemaForm
			context={{
				root,
				hints: PROFILE_HINTS,
				value,
				problems,
				onChange: (next) => {
					setValue(next)
					onValue?.(next)
				},
			}}
		/>
	)
}

describe('SchemaForm', () => {
	it('marks required fields and hides the schema version', () => {
		render(<Harness />)
		const offer = screen.getByLabelText(/Offre/)
		expect(offer.tagName).toBe('TEXTAREA')
		expect(screen.getByText('Offre').parentElement?.textContent).toContain('*')
		expect(screen.getAllByTitle('Obligatoire').length).toBeGreaterThan(5)
		expect(screen.queryByLabelText(/Version du schéma/)).toBeNull()
		expect(screen.getByLabelText('Langue des réponses')).toHaveProperty(
			'value',
			'fr',
		)
		expect(screen.queryByLabelText(/Seuil de qualification/)).toBeNull()
	})

	it('edits a list of strings as tags', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		const input = screen.getByLabelText('Adresses à privilégier')
		fireEvent.change(input, { target: { value: 'contact' } })
		fireEvent.keyDown(input, { key: 'Enter' })
		fireEvent.keyDown(input, { key: ',' })
		;(input as HTMLInputElement).value = 'sales'
		fireEvent.keyDown(input, { key: ',' })
		expect(seen.mock.calls.at(-1)?.[0]).toMatchObject({
			contact_ranking: { preferred_local_parts: ['contact', 'sales'] },
		})
		fireEvent.click(screen.getByLabelText('Retirer sales'))
		expect(seen.mock.calls.at(-1)?.[0]).toMatchObject({
			contact_ranking: { preferred_local_parts: ['contact'] },
		})
		expect(screen.queryByLabelText('Adresses à privilégier 1')).toBeNull()
	})

	it('uses tags for keywords and rows for objects', () => {
		const initial = setIn(
			defaultValue(root, root, PROFILE_HINTS),
			['agents'],
			[
				{
					type: 'search',
					id: 'a',
					name: 'A',
					enabled: true,
					providers: [],
					keywords: ['x', 'y'],
					exclusions: [],
				},
				{
					type: 'signal',
					id: 'b',
					name: 'B',
					enabled: true,
					source: {
						provider: 'dns_cname',
						patterns: [{ provider: 'Okta', target_suffix: 'okta.com' }],
					},
				},
			],
		)
		render(<Harness initial={initial} />)
		expect(screen.getByLabelText('Retirer x')).toBeTruthy()
		expect(screen.getByLabelText('Retirer y')).toBeTruthy()
		expect(screen.queryByLabelText('Mots-clés 1')).toBeNull()
		const row = document.querySelector<HTMLElement>(
			'[data-row="agents[1].source.patterns[0]"]',
		)
		expect(row).toBeTruthy()
		expect(
			within(row as HTMLElement).getByLabelText('Fournisseur'),
		).toHaveProperty('value', 'Okta')
		expect(
			within(row as HTMLElement).getByLabelText('Suffixe cible'),
		).toHaveProperty('value', 'okta.com')
		expect(screen.getByText('Ajouter un motif')).toBeTruthy()
	})

	it('marks the faulty chip and lists the message under the field', () => {
		const initial = setIn(
			defaultValue(root, root, PROFILE_HINTS),
			['agents'],
			[
				{
					type: 'search',
					id: 'a',
					name: 'A',
					enabled: true,
					providers: [],
					keywords: ['a', 'b', ''],
					exclusions: [],
				},
			],
		)
		render(
			<Harness
				initial={initial}
				problems={[
					{
						path: 'agents[0].keywords[2]',
						message: 'Ne doit pas être vide',
					},
				]}
			/>,
		)
		const chips = document.querySelectorAll<HTMLElement>('[data-tag]')
		expect(chips).toHaveLength(3)
		expect(chips[2]?.getAttribute('aria-invalid')).toBe('true')
		expect(chips[0]?.getAttribute('aria-invalid')).toBe('false')
		expect(screen.getByText('Ne doit pas être vide')).toBeTruthy()
	})

	it('adds a card for lists of objects', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByLabelText('Ajouter : Agents de scoring'))
		expect(screen.getByText('Agent de scoring 1')).toBeTruthy()
		const last = seen.mock.calls.at(-1)?.[0] as {
			scoring_agents: unknown[]
		}
		expect(last.scoring_agents).toHaveLength(1)
	})

	it('toggles optional sections and omits the key when off', () => {
		const initial = setIn(
			defaultValue(root, root, PROFILE_HINTS),
			['agents'],
			[
				{
					type: 'signal',
					id: 'a',
					name: 'A',
					enabled: true,
					source: {
						provider: 'http_probe',
						fingerprints: [
							{
								id: 'f',
								path: '/x',
								product: 'p',
								matchers: [],
								debt_markers: [],
							},
						],
						excluded_products: [],
					},
				},
			],
		)
		const seen = vi.fn()
		render(<Harness initial={initial} onValue={seen} />)
		const before = screen.queryAllByLabelText(/Expression régulière/).length
		fireEvent.click(screen.getByLabelText('Activer : Extracteur de version'))
		expect(screen.getAllByLabelText(/Expression régulière/)).toHaveLength(
			before + 1,
		)
		expect(seen.mock.calls.at(-1)?.[0]).toHaveProperty(
			'agents.0.source.fingerprints.0.version',
		)
		fireEvent.click(screen.getByLabelText('Activer : Extracteur de version'))
		const last = seen.mock.calls.at(-1)?.[0]
		expect(last).not.toHaveProperty('agents.0.source.fingerprints.0.version')
		expect(screen.queryAllByLabelText(/Expression régulière/)).toHaveLength(
			before,
		)
	})

	it('switches union variants and resets their fields', () => {
		const initial = setIn(
			defaultValue(root, root, PROFILE_HINTS),
			['agents'],
			[
				{
					type: 'signal',
					id: 'a',
					name: 'A',
					enabled: true,
					source: {
						provider: 'http_probe',
						fingerprints: [
							{
								id: 'a',
								path: '/',
								product: 'p',
								matchers: [{ kind: 'header', name: 'x', pattern: 'y' }],
							},
						],
					},
				},
			],
		)
		const seen = vi.fn()
		render(<Harness initial={initial} onValue={seen} />)
		expect(screen.getAllByLabelText(/^Nom/)).toHaveLength(2)
		fireEvent.change(screen.getByLabelText('Type : Condition'), {
			target: { value: '2' },
		})
		expect(screen.getAllByLabelText(/^Nom/)).toHaveLength(1)
		expect(screen.getByLabelText(/Expression régulière/)).toHaveProperty(
			'value',
			'',
		)
		expect(seen.mock.calls.at(-1)?.[0]).toMatchObject({
			agents: [
				{
					source: {
						fingerprints: [{ matchers: [{ kind: 'body', pattern: '' }] }],
					},
				},
			],
		})
	})

	it('shows a server error under the faulty field', () => {
		render(
			<Harness
				problems={[{ path: 'evidence.influence', message: 'must be positive' }]}
			/>,
		)
		const weight = screen.getByLabelText(/Influence des preuves/)
		const message = screen.getByText('must be positive')
		expect(weight.getAttribute('aria-invalid')).toBe('true')
		expect(weight.getAttribute('aria-describedby')).toBe(
			message.parentElement?.id,
		)
		const frame = weight.parentElement as HTMLElement
		expect(within(frame).getByText('must be positive')).toBe(message)
	})
})
