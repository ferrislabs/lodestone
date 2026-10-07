import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Calibration, Quality } from '#/pages/quality/lib/types'
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
	exclusions: [],
	exclusion_candidates: [],
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
			kind: 'aggregator',
			agent_id: null,
			agent_name: null,
			model: 'gros-modele',
			prompt_version: 'aggregator@f00',
			validated: 7,
			rejected: 3,
			escalated: false,
			n: 10,
			precision: 0.7,
		},
		{
			kind: 'qualification',
			agent_id: 'pertinence',
			agent_name: 'Pertinence métier',
			model: 'qwen3',
			prompt_version: 'qualification-agent@abc123',
			validated: 9,
			rejected: 1,
			escalated: false,
			n: 10,
			precision: 0.9,
		},
		{
			kind: 'qualification',
			agent_id: 'pertinence',
			agent_name: 'Pertinence métier',
			model: 'qwen3',
			prompt_version: 'qualification-agent@def456',
			validated: 3,
			rejected: 7,
			escalated: false,
			n: 10,
			precision: 0.3,
		},
		{
			kind: 'scoring',
			agent_id: 'score-global',
			agent_name: 'Score global',
			model: 'gemma_laya',
			prompt_version: 'scoring-agent@9a9a',
			validated: 8,
			rejected: 2,
			escalated: false,
			n: 10,
			precision: 0.8,
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
	exclusions: [],
	exclusion_candidates: [],
}

const CALIBRATION: Calibration = {
	required: 50,
	required_per_class: 10,
	profiles: [
		{
			profile_id: 'p1',
			name: 'Fintech',
			learned: true,
			agents: [
				{
					kind: 'scoring',
					agent_id: 'score-global',
					name: 'Score global',
					n: 80,
					positives: 30,
					brier_before: 0.3123,
					brier_after: 0.2,
					status: 'active',
					reliability: [
						{ n: 40, predicted: 0.2, observed: 0.1 },
						{ n: 40, predicted: 0.85, observed: 0.6 },
					],
					fitted_at: '2026-10-07T00:00:00Z',
				},
				{
					kind: 'qualification',
					agent_id: 'pertinence',
					name: 'Pertinence',
					n: 12,
					positives: 5,
					brier_before: 0.25,
					brier_after: null,
					status: 'insufficient',
					reliability: [{ n: 12, predicted: 0.5, observed: 0.4 }],
					fitted_at: null,
				},
			],
		},
		{
			profile_id: 'p2',
			name: 'Santé',
			learned: false,
			agents: [
				{
					kind: 'scoring',
					agent_id: 'score-sante',
					name: 'Score santé',
					n: 70,
					positives: 20,
					brier_before: 0.2,
					brier_after: null,
					status: 'disabled',
					reliability: [],
					fitted_at: null,
				},
			],
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
		copiedLeadId: null,
		copyFailed: false,
		onCopyExclusion: vi.fn(),
		calibration: null,
		calibrationRunning: false,
		calibrationNotice: null,
		calibrationError: null,
		onRecalibrate: vi.fn(),
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
			'Type',
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
		expect(within(aggregator).getAllByText('Agrégateur')).toHaveLength(2)
		expect(within(aggregator).getByText('gros-modele')).toBeTruthy()
		expect(within(aggregator).getByText('70 %')).toBeTruthy()
	})

	it('counts the precision of each scoring agent per version', () => {
		renderPage()
		const card = within(screen.getByLabelText('Précision par version'))
		const scoring = card.getByRole('row', { name: /scoring-agent@9a9a/ })
		expect(scoring.getAttribute('data-version-kind')).toBe('scoring')
		expect(within(scoring).getByText('Scoring')).toBeTruthy()
		expect(within(scoring).getByText('Score global')).toBeTruthy()
		expect(within(scoring).getByText('gemma_laya')).toBeTruthy()
		expect(within(scoring).getByText('80 %')).toBeTruthy()
		const qualification = card.getByRole('row', { name: /abc123/ })
		expect(within(qualification).getByText('Qualification')).toBeTruthy()
	})

	it('counts escalated runs on their own row with a badge', () => {
		const [aggregator, original] = FILLED.versions
		renderPage({
			quality: {
				...FILLED,
				versions: [
					aggregator,
					original,
					{ ...original, escalated: true, validated: 1, rejected: 1, n: 2 },
				],
			},
		})
		const card = within(screen.getByLabelText('Précision par version'))
		const rows = card.getAllByRole('row', { name: /abc123/ })
		expect(rows).toHaveLength(2)
		expect(within(rows[0]).queryByText('Escaladé')).toBeNull()
		expect(within(rows[1]).getByText('Escaladé')).toBeTruthy()
		expect(within(rows[1]).getByText('2')).toBeTruthy()
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

	it('counts the exclusions per agent and option', () => {
		renderPage({
			quality: {
				...FILLED,
				exclusions: [
					{
						agent_id: 'exclusions',
						agent_name: 'Exclusions',
						question_id: 'concurrent',
						option: 'oui',
						label: 'Concurrent',
						count: 7,
					},
					{
						agent_id: 'exclusions',
						agent_name: 'Exclusions',
						question_id: 'client_actuel',
						option: 'oui',
						label: 'Client actuel',
						count: 2,
					},
				],
			},
		})
		const card = within(screen.getByLabelText('Exclusions'))
		const rows = card.getAllByRole('listitem')
		expect(rows.map((row) => row.textContent)).toEqual([
			'Exclusions : Concurrent7',
			'Exclusions : Client actuel2',
		])
	})

	it('shows the exclusions card without decisions when leads were excluded', () => {
		renderPage({
			quality: {
				...EMPTY,
				exclusions: [
					{
						agent_id: 'exclusions',
						agent_name: 'Exclusions',
						question_id: 'concurrent',
						option: 'oui',
						label: 'Concurrent',
						count: 1,
					},
				],
			},
		})
		expect(screen.getByText(/Aucune décision enregistrée/)).toBeTruthy()
		expect(screen.getByLabelText('Exclusions')).toBeTruthy()
	})

	it('says so when no lead was excluded', () => {
		renderPage()
		expect(
			within(screen.getByLabelText('Exclusions')).getByText(
				'Aucun lead exclu par un agent sur cette période.',
			),
		).toBeTruthy()
		expect(screen.queryByLabelText("Ajouter à la liste d'exclusion")).toBeNull()
	})

	it('offers to copy the rejected leads into the exclusion lists', () => {
		const onCopyExclusion = vi.fn()
		const candidate = {
			lead_id: 'l1',
			domain: 'acme.test',
			legal_name: 'ACME SAS',
			reason: 'concurrent' as const,
			decided_at: '2026-02-01T00:00:00Z',
		}
		renderPage({
			quality: { ...FILLED, exclusion_candidates: [candidate] },
			onCopyExclusion,
		})
		const section = within(
			screen.getByLabelText("Ajouter à la liste d'exclusion"),
		)
		expect(section.getByText('ACME SAS')).toBeTruthy()
		expect(section.getByText(/Concurrent · liste « Concurrents »/)).toBeTruthy()
		expect(section.getByText(/Rien n'est modifié automatiquement/)).toBeTruthy()
		fireEvent.click(
			section.getByRole('button', {
				name: "Copier ACME SAS pour la liste d'exclusion",
			}),
		)
		expect(onCopyExclusion).toHaveBeenCalledWith(candidate)
	})

	it('confirms the copy and reports a clipboard failure', () => {
		const candidate = {
			lead_id: 'l1',
			domain: 'acme.test',
			legal_name: null,
			reason: 'deja_client' as const,
			decided_at: '2026-02-01T00:00:00Z',
		}
		renderPage({
			quality: { ...FILLED, exclusion_candidates: [candidate] },
			copiedLeadId: 'l1',
			copyFailed: true,
		})
		const section = within(
			screen.getByLabelText("Ajouter à la liste d'exclusion"),
		)
		expect(section.getByText('Copié')).toBeTruthy()
		expect(section.getByText('acme.test')).toBeTruthy()
		expect(section.getByText(/liste « Clients actuels »/)).toBeTruthy()
		expect(section.getByRole('alert').textContent).toContain('presse-papiers')
	})

	it('shows the calibration of every agent of every profile', () => {
		renderPage({ calibration: CALIBRATION })
		const card = within(screen.getByLabelText('Calibration'))
		const fintech = within(card.getByLabelText('Calibration de Fintech'))
		const scoring = fintech
			.getByText('Score global')
			.closest('tr') as HTMLElement
		expect(within(scoring).getByText('80')).toBeTruthy()
		expect(within(scoring).getByText('30')).toBeTruthy()
		expect(within(scoring).getByText('0,312')).toBeTruthy()
		expect(within(scoring).getByText('0,2')).toBeTruthy()
		expect(within(scoring).getByText('Active')).toBeTruthy()
		const qualification = fintech
			.getByText('Pertinence')
			.closest('tr') as HTMLElement
		expect(
			within(qualification).getByText('Pas assez de décisions (12/50)'),
		).toBeTruthy()
		expect(within(qualification).getByText('—')).toBeTruthy()
		const sante = within(card.getByLabelText('Calibration de Santé'))
		expect(sante.getByText('Désactivée')).toBeTruthy()
	})

	it('draws one reliability curve per agent with a diagonal reference', () => {
		renderPage({ calibration: CALIBRATION })
		const curve = screen.getByRole('img', {
			name: 'Courbe de fiabilité de Score global',
		})
		expect(curve.querySelector('[data-part="diagonal"]')).not.toBeNull()
		expect(curve.querySelectorAll('[data-part="bin"]')).toHaveLength(2)
		expect(
			curve.querySelector('[data-part="curve"]')?.getAttribute('points'),
		).toBe('20,90 85,40')
		expect(
			screen.getByRole('img', { name: 'Courbe de fiabilité de Pertinence' }),
		).toBeTruthy()
		expect(
			screen.queryByRole('img', { name: 'Courbe de fiabilité de Score santé' }),
		).toBeNull()
	})

	it('keeps only the selected profile in the calibration card', () => {
		renderPage({ calibration: CALIBRATION, profileId: 'p2' })
		const card = within(screen.getByLabelText('Calibration'))
		expect(card.queryByLabelText('Calibration de Fintech')).toBeNull()
		expect(card.getByLabelText('Calibration de Santé')).toBeTruthy()
	})

	it('asks for a recalculation and shows the outcome', () => {
		const props = renderPage({
			calibration: CALIBRATION,
			calibrationNotice: 'Recalcul programmé : bientôt.',
		})
		fireEvent.click(
			screen.getByRole('button', { name: 'Recalculer maintenant' }),
		)
		expect(props.onRecalibrate).toHaveBeenCalledTimes(1)
		expect(screen.getByText('Recalcul programmé : bientôt.')).toBeTruthy()
	})

	it('disables the button while the recalculation is being queued', () => {
		renderPage({ calibration: CALIBRATION, calibrationRunning: true })
		const button = screen.getByRole('button', {
			name: 'Recalculer maintenant',
		}) as HTMLButtonElement
		expect(button.disabled).toBe(true)
	})

	it('shows the calibration card even before any decision', () => {
		renderPage({ quality: EMPTY, calibration: CALIBRATION })
		expect(screen.getByLabelText('Calibration')).toBeTruthy()
	})

	it('shows a calibration error', () => {
		renderPage({
			calibration: CALIBRATION,
			calibrationError: 'Impossible de programmer le recalcul',
		})
		expect(
			within(screen.getByLabelText('Calibration')).getByRole('alert')
				.textContent,
		).toBe('Impossible de programmer le recalcul')
	})
})
