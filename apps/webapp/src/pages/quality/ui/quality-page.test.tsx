import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { SuggestionOutcome } from '#/pages/quality/lib/sourcing'
import type {
	AgentYield,
	Calibration,
	KeywordSuggestion,
	Quality,
	TriageRecall,
} from '#/pages/quality/lib/types'
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
	triage_recall: [],
	sourcing_yield: [],
	keyword_suggestions: [],
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
	triage_recall: [],
	sourcing_yield: [],
	keyword_suggestions: [],
}

const RECALL: TriageRecall[] = [
	{
		profile_id: 'p1',
		agent: 'offres-france',
		reason: 'no_keyword',
		dropped: 120,
		reviewed: 20,
		should_have_kept: 3,
		share: 0.15,
	},
	{
		profile_id: 'p2',
		agent: null,
		reason: 'filter_country',
		dropped: 40,
		reviewed: 0,
		should_have_kept: 0,
		share: null,
	},
]

const YIELD: AgentYield[] = [
	{
		profile_id: 'p1',
		agent_id: 'offres-france',
		agent_name: 'Offres France',
		leads: 12,
		verdicts: { reliable: 5, to_verify: 4, discarded: 2, none: 1 },
		validated: 6,
		rejected: 3,
		precision: 0.6667,
		keywords: [
			{
				keyword: 'cloud',
				leads: 8,
				exclusive_leads: 3,
				verdicts: { reliable: 3, to_verify: 2, discarded: 1, none: 2 },
				validated: 4,
				rejected: 1,
				precision: 0.8,
			},
			{
				keyword: 'devops',
				leads: 5,
				exclusive_leads: 0,
				verdicts: { reliable: 1, to_verify: 1, discarded: 0, none: 3 },
				validated: 0,
				rejected: 0,
				precision: null,
			},
		],
	},
	{
		profile_id: 'p2',
		agent_id: 'wttj',
		agent_name: null,
		leads: 3,
		verdicts: { reliable: 0, to_verify: 0, discarded: 0, none: 3 },
		validated: 0,
		rejected: 0,
		precision: null,
		keywords: [],
	},
]

const SUGGESTIONS: KeywordSuggestion[] = [
	{
		profile_id: 'p1',
		agent_id: 'offres-france',
		action: 'remove_keyword',
		word: 'stage',
		decided: 6,
		rejected: 6,
	},
	{
		profile_id: 'p1',
		agent_id: 'offres-france',
		action: 'add_exclusion',
		word: 'alternance',
		decided: 4,
		rejected: 4,
	},
]

const CALIBRATION: Calibration = {
	required: 50,
	required_per_class: 10,
	profiles: [
		{
			profile_id: 'p1',
			name: 'Fintech',
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

function cellTexts(row: HTMLElement): string[] {
	return within(row)
		.getAllByRole('cell')
		.map((cell) => (cell.textContent ?? '').replace(/\s+/g, ' '))
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
		onReviewTriage: vi.fn(),
		onOpenDropped: vi.fn(),
		expandedAgents: [] as string[],
		onToggleAgent: vi.fn(),
		suggestions: [] as KeywordSuggestion[],
		suggestionOutcomes: {} as Record<string, SuggestionOutcome>,
		onApplySuggestion: vi.fn(),
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

	it('shows the recall of the triage per agent and reason', () => {
		renderPage({ quality: { ...FILLED, triage_recall: RECALL } })
		const card = within(screen.getByLabelText('Rappel du tri'))
		expect(card.getByText('Rappel du tri')).toBeTruthy()
		expect(
			card.getByText(
				"Annonces écartées par le tri par mots-clés sur la période, et part de celles qu'un opérateur a jugées à garder.",
			),
		).toBeTruthy()
		const first = card.getByRole('row', { name: /Aucun mot-clé/ })
		expect(within(first).getByText('offres-france')).toBeTruthy()
		expect(within(first).getByText('Fintech')).toBeTruthy()
		expect(within(first).getByText('120')).toBeTruthy()
		expect(within(first).getByText('20')).toBeTruthy()
		expect(within(first).getByText('3')).toBeTruthy()
		expect(within(first).getByText('15 %')).toBeTruthy()
		const second = card.getByRole('row', { name: /Filtre pays/ })
		expect(
			within(second).getByText("Offres d'emploi de l'entreprise"),
		).toBeTruthy()
		expect(within(second).getByText('Santé')).toBeTruthy()
		expect(within(second).getByText('40')).toBeTruthy()
		expect(within(second).getByText('—')).toBeTruthy()
	})

	it('shows the recall of the triage even before any decision', () => {
		renderPage({ quality: { ...EMPTY, triage_recall: RECALL } })
		expect(screen.getByText(/Aucune décision enregistrée/)).toBeTruthy()
		expect(
			within(screen.getByLabelText('Rappel du tri')).getAllByRole('row'),
		).toHaveLength(3)
	})

	it('hides the recall of the triage when nothing was dropped or while loading', () => {
		renderPage()
		expect(screen.queryByLabelText('Rappel du tri')).toBeNull()
		cleanup()
		renderPage({
			quality: { ...EMPTY, triage_recall: RECALL },
			state: 'loading',
		})
		expect(screen.queryByLabelText('Rappel du tri')).toBeNull()
	})

	it('opens a sample of the dropped announcements of a recall row', () => {
		const props = renderPage({ quality: { ...FILLED, triage_recall: RECALL } })
		const second = screen.getByRole('row', { name: /Filtre pays/ })
		fireEvent.click(
			within(second).getByRole('button', { name: 'Revoir un échantillon' }),
		)
		expect(props.onReviewTriage).toHaveBeenCalledWith(RECALL[1])
	})

	it('links to the review of the dropped announcements with the chosen profile', () => {
		const props = renderPage({ profileId: 'p1' })
		const link = screen.getByRole('link', {
			name: 'Revoir les annonces écartées',
		})
		expect(link.getAttribute('href')).toBe('/quality/dropped?profile=p1')
		fireEvent.click(link)
		expect(props.onOpenDropped).toHaveBeenCalledWith('p1')
		cleanup()
		renderPage({ profileId: '' })
		expect(
			screen
				.getByRole('link', { name: 'Revoir les annonces écartées' })
				.getAttribute('href'),
		).toBe('/quality/dropped')
	})

	it('lists the yield of each sourcing agent with its verdicts and decisions', () => {
		renderPage({ quality: { ...FILLED, sourcing_yield: YIELD } })
		const card = within(
			screen.getByLabelText('Rendement des agents de recherche'),
		)
		expect(
			card.getByText(
				'Leads trouvés par chaque agent de recherche et par mot-clé sur la période, avec leurs verdicts et les décisions humaines.',
			),
		).toBeTruthy()
		expect(
			card.getAllByRole('columnheader').map((cell) => cell.textContent),
		).toEqual([
			'Mots-clés',
			'Agent',
			'Leads',
			'Fiable',
			'À vérifier',
			'Écarté',
			'Validés',
			'Rejetés',
			'Précision',
		])
		const first = card.getByRole('row', { name: /Offres France/ })
		expect(cellTexts(first)).toEqual([
			'',
			'Offres FranceFintech',
			'12',
			'5',
			'4',
			'2',
			'6',
			'3',
			'67 %',
		])
		const second = card.getByRole('row', { name: /wttj/ })
		expect(
			within(second).getByText('Absent de la révision actuelle'),
		).toBeTruthy()
		expect(within(second).getByText('Santé')).toBeTruthy()
		expect(within(second).getAllByText('—')).toHaveLength(1)
	})

	it('names the profile under the agent only when several profiles are covered', () => {
		renderPage({ quality: { ...FILLED, sourcing_yield: [YIELD[0]] } })
		const row = screen.getByRole('row', { name: /Offres France/ })
		expect(within(row).queryByText('Fintech')).toBeNull()
	})

	it('hides the yield of the sourcing agents when there is none or while loading', () => {
		renderPage()
		expect(
			screen.queryByLabelText('Rendement des agents de recherche'),
		).toBeNull()
		cleanup()
		renderPage({
			quality: { ...EMPTY, sourcing_yield: YIELD },
			state: 'loading',
		})
		expect(
			screen.queryByLabelText('Rendement des agents de recherche'),
		).toBeNull()
	})

	it('shows the yield of the sourcing agents even before any decision', () => {
		renderPage({ quality: { ...EMPTY, sourcing_yield: YIELD } })
		expect(screen.getByText(/Aucune décision enregistrée/)).toBeTruthy()
		expect(
			screen.getByLabelText('Rendement des agents de recherche'),
		).toBeTruthy()
	})

	it('asks to open and close the keywords of an agent', () => {
		const props = renderPage({ quality: { ...FILLED, sourcing_yield: YIELD } })
		const toggle = screen.getByRole('button', {
			name: 'Mots-clés de Offres France',
		})
		expect(toggle.getAttribute('aria-expanded')).toBe('false')
		expect(screen.queryByRole('row', { name: /cloud/ })).toBeNull()
		fireEvent.click(toggle)
		expect(props.onToggleAgent).toHaveBeenCalledWith('p1:offres-france')
	})

	it('shows the keywords of an opened agent with their exclusive leads', () => {
		renderPage({
			quality: { ...FILLED, sourcing_yield: YIELD },
			expandedAgents: ['p1:offres-france'],
		})
		const toggle = screen.getByRole('button', {
			name: 'Mots-clés de Offres France',
		})
		expect(toggle.getAttribute('aria-expanded')).toBe('true')
		const cloud = screen.getByRole('row', { name: /cloud/ })
		expect(cellTexts(cloud)).toEqual([
			'',
			'cloud',
			'8dont 3 seul',
			'3',
			'2',
			'1',
			'4',
			'1',
			'80 %',
		])
		expect(within(cloud).getByTitle('dont 3 par ce seul mot-clé')).toBeTruthy()
		const devops = screen.getByRole('row', { name: /devops/ })
		expect(within(devops).queryByText(/dont/)).toBeNull()
		expect(within(devops).getByText('—')).toBeTruthy()
		expect(
			screen
				.getByRole('button', { name: 'Mots-clés de wttj' })
				.getAttribute('aria-expanded'),
		).toBe('false')
	})

	it('says so when an opened agent has no keyword', () => {
		renderPage({
			quality: { ...FILLED, sourcing_yield: YIELD },
			expandedAgents: ['p2:wttj'],
		})
		expect(screen.getByText(/Aucun mot-clé n'a trouvé de lead/)).toBeTruthy()
	})

	it('words both kinds of keyword suggestions', () => {
		renderPage({
			quality: { ...FILLED, sourcing_yield: YIELD },
			suggestions: SUGGESTIONS,
		})
		const card = within(screen.getByLabelText('Suggestions de mots-clés'))
		expect(
			card.getByText(
				"Règles fixes appliquées aux décisions humaines. Rien n'est modifié sans votre clic : appliquer crée une nouvelle révision du profil.",
			),
		).toBeTruthy()
		expect(
			card.getByText(
				'Retirer le mot-clé « stage » de Offres France : 6 leads décidés, aucun validé.',
			),
		).toBeTruthy()
		expect(
			card.getByText(
				'Ajouter « alternance » aux exclusions de Offres France : présent dans 4 leads rejetés, aucun validé.',
			),
		).toBeTruthy()
		expect(card.getAllByRole('button', { name: 'Appliquer' })).toHaveLength(2)
	})

	it('applies the suggestion of the clicked line', () => {
		const props = renderPage({ suggestions: SUGGESTIONS })
		const second = screen
			.getByText(/Ajouter « alternance »/)
			.closest('li') as HTMLElement
		fireEvent.click(within(second).getByRole('button', { name: 'Appliquer' }))
		expect(props.onApplySuggestion).toHaveBeenCalledTimes(1)
		expect(props.onApplySuggestion).toHaveBeenCalledWith(SUGGESTIONS[1])
	})

	it('falls back to the agent id when the yield does not know the agent', () => {
		renderPage({ suggestions: SUGGESTIONS })
		expect(
			screen.getByText(/Retirer le mot-clé « stage » de offres-france :/),
		).toBeTruthy()
	})

	it('disables the button of a suggestion being applied', () => {
		renderPage({
			suggestions: SUGGESTIONS,
			suggestionOutcomes: {
				'p1:offres-france:remove_keyword:stage': { status: 'pending' },
			},
		})
		const pending = screen.getByRole('button', {
			name: 'Application…',
		}) as HTMLButtonElement
		expect(pending.disabled).toBe(true)
		expect(screen.getAllByRole('button', { name: 'Appliquer' })).toHaveLength(1)
	})

	it('confirms an applied suggestion with the new revision and drops its button', () => {
		renderPage({
			suggestions: SUGGESTIONS,
			suggestionOutcomes: {
				'p1:offres-france:remove_keyword:stage': {
					status: 'applied',
					revision: 4,
				},
			},
		})
		const line = screen
			.getByText(/Retirer le mot-clé « stage »/)
			.closest('li') as HTMLElement
		expect(within(line).getByText('Révision 4 créée')).toBeTruthy()
		expect(within(line).queryByRole('button')).toBeNull()
	})

	it('shows the error of a suggestion under its line and keeps the button', () => {
		renderPage({
			suggestions: SUGGESTIONS,
			suggestionOutcomes: {
				'p1:offres-france:add_exclusion:alternance': {
					status: 'failed',
					message: 'Le profil a changé de révision',
				},
			},
		})
		const line = screen
			.getByText(/Ajouter « alternance »/)
			.closest('li') as HTMLElement
		expect(within(line).getByRole('alert').textContent).toBe(
			'Le profil a changé de révision',
		)
		expect(within(line).getByRole('button', { name: 'Appliquer' })).toBeTruthy()
	})

	it('names the profile of a suggestion only when several profiles are covered', () => {
		const other: KeywordSuggestion = {
			...SUGGESTIONS[0],
			profile_id: 'p2',
			agent_id: 'wttj',
		}
		renderPage({ suggestions: [SUGGESTIONS[0], other] })
		const card = within(screen.getByLabelText('Suggestions de mots-clés'))
		expect(card.getByText('Fintech')).toBeTruthy()
		expect(card.getByText('Santé')).toBeTruthy()
		cleanup()
		renderPage({ suggestions: SUGGESTIONS })
		expect(
			within(screen.getByLabelText('Suggestions de mots-clés')).queryByText(
				'Fintech',
			),
		).toBeNull()
	})

	it('hides the keyword suggestions when there are none or while loading', () => {
		renderPage()
		expect(screen.queryByLabelText('Suggestions de mots-clés')).toBeNull()
		cleanup()
		renderPage({ quality: null, state: 'loading', suggestions: SUGGESTIONS })
		expect(screen.queryByLabelText('Suggestions de mots-clés')).toBeNull()
	})

	it('shows the keyword suggestions even before any decision', () => {
		renderPage({ quality: EMPTY, suggestions: SUGGESTIONS })
		expect(screen.getByText(/Aucune décision enregistrée/)).toBeTruthy()
		expect(screen.getByLabelText('Suggestions de mots-clés')).toBeTruthy()
	})
})
