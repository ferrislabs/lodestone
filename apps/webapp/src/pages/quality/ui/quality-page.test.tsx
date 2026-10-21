import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Quality } from '#/pages/quality/lib/types'
import { QualityPage } from './quality-page'

afterEach(cleanup)

const BUCKETS = Array.from({ length: 10 }, (_, index) => ({
	from: index * 10,
	to: index * 10 + 10,
	validated: index === 7 ? 4 : 0,
	rejected: index === 2 ? 3 : 0,
}))

const EMPTY: Quality = {
	decisions: {
		validated: 0,
		rejected: 0,
		drafts_approved: 0,
		drafts_rejected: 0,
	},
	verdicts: [],
	discarded: { discarded: 0, total: 0, share: null },
	confidence: BUCKETS.map((bucket) => ({
		...bucket,
		validated: 0,
		rejected: 0,
	})),
	agents: [],
	versions: [],
	profiles: [],
}

const FILLED: Quality = {
	decisions: {
		validated: 12,
		rejected: 8,
		drafts_approved: 5,
		drafts_rejected: 2,
	},
	verdicts: [
		{ verdict: 'fiable', validated: 9, rejected: 1, n: 10, precision: 0.9 },
		{ verdict: 'a_verifier', validated: 3, rejected: 3, n: 6, precision: 0.5 },
		{ verdict: 'ecarte', validated: 0, rejected: 4, n: 4, precision: 0 },
	],
	discarded: { discarded: 25, total: 100, share: 0.25 },
	confidence: BUCKETS,
	agents: [
		{
			agent_id: 'pertinence',
			name: 'Pertinence métier',
			mean_validated: 0.82,
			mean_rejected: 0.31,
			separation: 0.51,
			validated: 12,
			rejected: 8,
		},
	],
	versions: [
		{
			agent_id: null,
			agent_name: null,
			model: 'gros-modele',
			prompt_version: 'aggregator@f00',
			validated: 7,
			rejected: 3,
			n: 10,
			precision: 0.7,
		},
		{
			agent_id: 'pertinence',
			agent_name: 'Pertinence métier',
			model: 'qwen3',
			prompt_version: 'qualification-agent@abc123',
			validated: 9,
			rejected: 1,
			n: 10,
			precision: 0.9,
		},
		{
			agent_id: 'pertinence',
			agent_name: 'Pertinence métier',
			model: 'qwen3',
			prompt_version: 'qualification-agent@def456',
			validated: 3,
			rejected: 7,
			n: 10,
			precision: 0.3,
		},
	],
	profiles: [
		{
			profile_id: 'p1',
			name: 'Fintech',
			validated: 12,
			rejected: 8,
			precision: 0.6,
			current: { reliable_from: 70, discard_below: 30 },
			suggestion: {
				ready: true,
				labelled: 20,
				required: 20,
				reliable_from: 65,
				discard_below: 25,
			},
			rejection_reasons: [
				{ reason: 'hors_cible', count: 5 },
				{ reason: 'hebergeur_agence', count: 2 },
				{ reason: 'autre', count: 1 },
			],
		},
		{
			profile_id: 'p2',
			name: 'Santé',
			validated: 1,
			rejected: 2,
			precision: 0.33,
			current: { reliable_from: 70, discard_below: 30 },
			suggestion: {
				ready: false,
				labelled: 3,
				required: 20,
				reliable_from: null,
				discard_below: null,
			},
			rejection_reasons: [],
		},
	],
}

function renderPage(
	overrides: Partial<Parameters<typeof QualityPage>[0]> = {},
) {
	const props = {
		quality: FILLED,
		state: 'ready' as const,
		errorMessage: null,
		profiles: [
			{ id: 'p1', name: 'Fintech', lead_count: 3, active: true },
			{ id: 'p2', name: 'Santé', lead_count: 1, active: true },
		],
		profileId: '',
		onProfileChange: vi.fn(),
		period: '30' as const,
		onPeriodChange: vi.fn(),
		onOpenProfile: vi.fn(),
		...overrides,
	}
	render(<QualityPage {...props} />)
	return props
}

describe('QualityPage', () => {
	it('shows an empty state when nothing was decided', () => {
		renderPage({ quality: EMPTY })
		expect(screen.getByText(/Aucune décision enregistrée/)).toBeTruthy()
		expect(screen.queryByText('Précision par verdict')).toBeNull()
	})

	it('shows a loading skeleton and an error without numbers', () => {
		renderPage({ quality: null, state: 'loading' })
		expect(screen.queryByText('Décisions')).toBeNull()
		cleanup()
		renderPage({ quality: null, state: 'error', errorMessage: 'Panne' })
		expect(screen.getByRole('alert').textContent).toBe('Panne')
	})

	it('shows the decision counts', () => {
		renderPage()
		const card = within(screen.getByLabelText('Décisions'))
		expect(card.getByText('Leads validés').nextSibling?.textContent).toBe('12')
		expect(card.getByText('Leads rejetés').nextSibling?.textContent).toBe('8')
		expect(
			card.getByText('Brouillons approuvés').nextSibling?.textContent,
		).toBe('5')
		expect(card.getByText('Brouillons rejetés').nextSibling?.textContent).toBe(
			'2',
		)
	})

	it('shows the precision per verdict with the sample size', () => {
		renderPage()
		const card = within(screen.getByLabelText('Précision par verdict'))
		const row = card.getByRole('row', { name: /Fiable/ })
		expect(within(row).getByText('90 %')).toBeTruthy()
		expect(within(row).getByText('10')).toBeTruthy()
		expect(
			within(card.getByRole('row', { name: /Écarté/ })).getByText('0 %'),
		).toBeTruthy()
	})

	it('shows the share of discarded leads', () => {
		renderPage()
		const card = within(screen.getByLabelText("Taux d'écartés"))
		expect(card.getByText('25 %')).toBeTruthy()
		expect(card.getByText(/25 sur 100 leads/)).toBeTruthy()
	})

	it('draws ten buckets for validated and rejected leads', () => {
		renderPage()
		const histogram = screen.getByLabelText('Distribution de la confiance')
		expect(within(histogram).getAllByRole('listitem')).toHaveLength(10)
		const bucket = within(histogram).getByLabelText(
			'70–80 % : 4 validés, 0 rejetés',
		)
		expect(bucket.querySelector('[data-bar="validated"]')).toHaveProperty(
			'style.height',
			'100%',
		)
		expect(
			within(histogram).getByLabelText('20–30 % : 0 validés, 3 rejetés'),
		).toBeTruthy()
	})

	it('lists the agents with their separation', () => {
		renderPage()
		const row = within(screen.getByLabelText('Agents')).getByRole('row', {
			name: /Pertinence métier/,
		})
		expect(within(row).getByText('pertinence')).toBeTruthy()
		expect(within(row).getByText('82 %')).toBeTruthy()
		expect(within(row).getByText('31 %')).toBeTruthy()
		expect(within(row).getByText('+51 pts')).toBeTruthy()
	})

	it('compares the precision by agent, model and prompt version', () => {
		renderPage()
		const card = within(screen.getByLabelText('Précision par version'))
		const headers = card
			.getAllByRole('columnheader')
			.map((cell) => cell.textContent)
		expect(headers).toEqual([
			'Agent',
			'Modèle',
			'Version du prompt',
			'n',
			'Précision',
		])
		const first = card.getByRole('row', { name: /abc123/ })
		expect(within(first).getByText('Pertinence métier')).toBeTruthy()
		expect(within(first).getByText('qwen3')).toBeTruthy()
		expect(within(first).getByText('90 %')).toBeTruthy()
		expect(within(first).getByText('10')).toBeTruthy()
		const second = card.getByRole('row', { name: /def456/ })
		expect(within(second).getByText('30 %')).toBeTruthy()
		const aggregator = card.getByRole('row', { name: /aggregator@f00/ })
		expect(within(aggregator).getByText('Agrégateur')).toBeTruthy()
		expect(within(aggregator).getByText('gros-modele')).toBeTruthy()
		expect(within(aggregator).getByText('70 %')).toBeTruthy()
	})

	it('says so when no version was recorded', () => {
		renderPage({ quality: { ...FILLED, versions: [] } })
		expect(
			within(screen.getByLabelText('Précision par version')).getByText(
				/Aucune version enregistrée/,
			),
		).toBeTruthy()
	})

	it('shows the rejection reasons per profile and skips profiles without any', () => {
		renderPage()
		const card = within(screen.getByLabelText('Raisons de rejet'))
		const fintech = within(card.getByLabelText('Raisons de rejet de Fintech'))
		const items = fintech
			.getAllByRole('listitem')
			.map((item) => item.textContent)
		expect(items).toEqual([
			'Hors cible5',
			'Hébergeur ou agence (le signal concerne un client)2',
			'Autre1',
		])
		expect(card.queryByLabelText('Raisons de rejet de Santé')).toBeNull()
	})

	it('says so when no rejection reason was recorded', () => {
		renderPage({
			quality: {
				...FILLED,
				profiles: FILLED.profiles.map((profile) => ({
					...profile,
					rejection_reasons: [],
				})),
			},
		})
		expect(
			within(screen.getByLabelText('Raisons de rejet')).getByText(
				/Aucune raison de rejet enregistrée/,
			),
		).toBeTruthy()
	})

	it('words the suggestions and never applies them', () => {
		const props = renderPage()
		const ready = within(screen.getByLabelText('Suggestions pour Fintech'))
		expect(
			ready.getByText('Seuil « Fiable » suggéré : 65 % (actuel 70 %)'),
		).toBeTruthy()
		expect(
			ready.getByText('Seuil « Écarté » suggéré : 25 % (actuel 30 %)'),
		).toBeTruthy()
		const short = within(screen.getByLabelText('Suggestions pour Santé'))
		expect(short.getByText('Pas assez de décisions (3/20)')).toBeTruthy()
		expect(screen.getByText(/ne modifie jamais un seuil/)).toBeTruthy()

		fireEvent.click(
			ready.getByRole('button', { name: "Ouvrir l'onglet Qualification" }),
		)
		expect(props.onOpenProfile).toHaveBeenCalledWith('p1')
	})

	it('reports the period and profile choices', () => {
		const props = renderPage()
		fireEvent.click(screen.getByLabelText('90 j'))
		expect(props.onPeriodChange).toHaveBeenCalledWith('90')
		fireEvent.change(screen.getByLabelText('Profil'), {
			target: { value: 'p2' },
		})
		expect(props.onProfileChange).toHaveBeenCalledWith('p2')
	})
})
