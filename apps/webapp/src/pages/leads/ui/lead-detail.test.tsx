import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { formatDateTime } from '#/pages/leads/lib/format'
import type { LeadDetailData, ScoringResult } from '#/pages/leads/lib/types'
import { LeadDetail } from './lead-detail'

afterEach(cleanup)

const provenance = {
	provider_id: 'prov-1',
	provider_name: 'Laya',
	model: 'gemma_laya',
	prompt_version: 'qualification-agent@abc',
	generation: { temperature: 0.2, max_tokens: 2048 },
	samples: 1,
}

const scoringResult: ScoringResult = {
	agent_id: 'score-global',
	agent_name: 'Score global',
	agent_description: 'Décide du score et du verdict.',
	weight: 5,
	provider_id: 'prov-2',
	provider_name: 'Synthèse',
	model: 'gemma_laya',
	provenance: {
		...provenance,
		provider_name: 'Synthèse',
		prompt_version: 'scoring-agent@abc',
	},
	status: 'valid',
	score: 72,
	verdict: {
		choice: 'a_verifier',
		declared_choice: 'fiable',
		confidence: 60,
		probabilities: { fiable: 30, a_verifier: 60, ecarte: 10 },
	},
	reasons: ['Un seul signal.', 'Entreprise ciblée.'],
	criteria: [
		{
			question_id: 'adequation_offre',
			wording: "Le lead correspond-il à la cible de l'offre ?",
			choice: 'forte',
			choice_label: 'Forte',
			evidence: 'expose Keycloak 18',
			evidence_source: 'https://acme.test/realms/master',
			grounded: true,
			original_choice: null,
			original_choice_label: null,
			confidence: 0.8,
			probabilities: [
				{ value: 'forte', label: 'Forte', probability: 0.8 },
				{ value: 'inconnu', label: 'Inconnu', probability: 0.2 },
			],
		},
		{
			question_id: 'solidite_preuves',
			wording: 'Les preuves sont-elles solides ?',
			choice: 'inconnu',
			choice_label: 'Inconnu',
			evidence: 'phrase inventée',
			evidence_source: null,
			grounded: false,
			original_choice: 'solide',
			original_choice_label: 'Solide',
			confidence: 0.5,
			probabilities: [{ value: 'inconnu', label: 'Inconnu', probability: 1 }],
		},
	],
	error: null,
	created_at: '2026-02-01T00:00:00Z',
}

const detail: LeadDetailData = {
	lead: {
		id: 'l1',
		profile: { id: 'p1', name: 'Fintech' },
		domain: 'acme.test',
		status: 'qualified',
		exclusions: [],
		grouped_leads: [],
		score: 42.5,
		first_seen_at: '2026-01-01T00:00:00Z',
		last_seen_at: '2026-02-01T00:00:00Z',
		updated_at: '2026-02-01T00:00:00Z',
		confidence: 72.5,
		verdict: 'a_verifier',
		llm_score: 68,
		verdict_probabilities: { fiable: 20, a_verifier: 72.5, ecarte: 7.5 },
		verdict_source: 'scoring',
		indicator_confidence: 41.5,
		indicator_cap: 'non_production',
		opinion: 92.9,
		evidence: {
			strength: 35,
			components: {
				volume: 28.4,
				diversity: 50,
				environment: 0,
				strength: 24.2,
			},
			signal_count: 1,
			independent_signals: 1,
			decayed_signals: 0.5,
			source_count: 1,
			kind_count: 1,
			signal_score: 12.1,
			only_non_production: true,
		},
		verdict_reason: null,
		qualified_at: '2026-02-01T00:00:00Z',
	},
	scoring_results: [],
	agent_failures: [],
	agent_stops: [],
	enrichment: { status: 'disabled', steps: [] },
	qualification_results: [
		{
			agent_id: 'pertinence',
			agent_name: 'Pertinence',
			agent_description: "La pertinence du lead pour l'offre.",
			summary: 'Le lead exploite un Keycloak ancien et recrute.',
			provider_id: 'prov-1',
			provider_name: 'Laya',
			model: 'gemma_laya',
			provenance,
			status: 'valid',
			agent_score: 0.625,
			answers: [
				{
					question_id: 'pertinence',
					wording: "Ce lead est-il pertinent pour l'offre ?",
					choice: 'moyenne',
					choice_label: 'Moyenne',
					evidence: 'expose Keycloak 18 en production',
					evidence_source: 'https://acme.test/realms/master',
					grounded: true,
					original_choice: null,
					original_choice_label: null,
					confidence: 0.8,
					probabilities: [
						{ value: 'forte', label: 'Forte', probability: 0.25 },
						{ value: 'moyenne', label: 'Moyenne', probability: 0.5 },
						{ value: 'faible', label: 'Faible', probability: 0.25 },
					],
				},
			],
			error: null,
			escalated: false,
			created_at: '2026-02-01T00:00:00Z',
		},
		{
			agent_id: 'maturite',
			agent_name: 'Maturité',
			agent_description: 'La maturité technique du lead.',
			summary: null,
			provider_id: null,
			provider_name: null,
			model: '',
			provenance: { ...provenance, model: '' },
			status: 'invalid',
			agent_score: null,
			answers: [],
			error: 'fournisseur indisponible',
			escalated: false,
			created_at: '2026-02-01T00:00:00Z',
		},
	],
	facts: {
		hosts: [
			{ host: 'login.dev.acme.test', environment: 'development', signals: 1 },
		],
		distinct_hosts: 1,
		production_hosts: 0,
		production_share: 0,
		only_non_production: true,
		distinct_subdomains: 3,
		signal_count: 1,
		independent_signals: 1,
		kinds: ['hiring_page'],
		sibling_domains: [],
		source_count: 1,
		sources: ['crawler'],
		first_seen_at: '2026-01-01T00:00:00Z',
		last_seen_at: '2026-02-01T00:00:00Z',
		probable_country: { code: 'FR', name: 'France' },
	},
	signals: [
		{
			id: 's1',
			kind: 'hiring_page',
			value: 'careers',
			sentence: 'acme.test publie une page de recrutement (careers)',
			source: 'crawler',
			agent: 'ats-fr',
			provenance: 'https://acme.test/careers',
			strength: 0.8,
			observed_at: '2026-02-01T00:00:00Z',
			raw: {},
		},
	],
	contacts: [
		{
			id: 'c1',
			address: 'jane@acme.test',
			method: 'page_scrape',
			kind: 'nominative',
			confidence: 0.9,
			provenance: 'https://acme.test/team',
			found_at: '2026-02-01T00:00:00Z',
		},
	],
	assessments: [
		{
			id: 'a1',
			profile_revision: 3,
			summary: 'Growing team',
			verdict_narrative: 'Both agents agree the lead is relevant.',
			approach_angle: 'Offer tooling',
			evidence: [
				{ quote: 'We are hiring', provenance: 'https://acme.test/careers' },
			],
			evidence_verified: true,
			quality_warnings: [],
			model: 'model-x',
			prompt_version: 'v2',
			provenance: {
				...provenance,
				model: 'model-x',
				prompt_version: 'v2',
			},
			draft: { subject: 'Hello', body: 'Body text', status: 'pending' },
			created_at: '2026-02-01T00:00:00Z',
		},
	],
}

function renderDetail(
	overrides: Partial<Parameters<typeof LeadDetail>[0]> = {},
) {
	return render(
		<LeadDetail
			state="ready"
			errorMessage={null}
			detail={detail}
			nextStatuses={['validated', 'rejected', 'new']}
			onBack={() => {}}
			onOpenLead={() => {}}
			onChangeStatus={() => {}}
			rejectionDialog={<div>rejection-dialog</div>}
			onOpenDraft={() => {}}
			onRetrySummary={() => {}}
			retryingSummary={false}
			retrySummaryError={null}
			onRetryAgents={() => {}}
			retryingAgents={false}
			retryAgentsError={null}
			onRefreshCompany={() => {}}
			refreshingCompany={false}
			refreshCompanyError={null}
			changing={false}
			changeError={null}
			{...overrides}
		/>,
	)
}

const scoringFailure = {
	kind: 'scoring' as const,
	agent_id: 'score-global',
	agent_name: 'Score global',
	attempts: 2,
	failed: false,
	error:
		"Échec de l'appel au modèle : llm provider answered with status 400: reasoning_effort is not enabled for this model",
	next_attempt_at: '2026-02-01T00:05:00Z',
}

const pendingScoring = {
	...detail.lead,
	verdict: null,
	confidence: null,
	llm_score: null,
	verdict_probabilities: null,
	verdict_reason: 'awaiting_scoring' as const,
}

describe('LeadDetail', () => {
	it('shows a scoring call failure with its provider message and the next attempt', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: pendingScoring,
				agent_failures: [scoringFailure],
			},
		})
		const notice = container.querySelector(
			'[data-section="scoring"] [data-agent-failure="scoring"]',
		)
		expect(notice?.getAttribute('data-failed')).toBe('false')
		expect(notice?.textContent).toContain('Agent « Score global »')
		expect(notice?.textContent).toContain(
			'reasoning_effort is not enabled for this model',
		)
		expect(
			notice?.querySelector('[data-part="failure-status"]')?.textContent,
		).toMatch(/^Nouvelle tentative : /)
		expect(notice?.querySelector('button')).toBeNull()
		expect(
			container.querySelector('[data-part="combined"] [data-verdict-reason]')
				?.textContent,
		).toMatch(/^En attente : appel au modèle en échec, nouvelle tentative /)
	})

	it('offers to retry once the agents are failed', () => {
		const onRetryAgents = vi.fn()
		const { container } = renderDetail({
			onRetryAgents,
			detail: {
				...detail,
				lead: pendingScoring,
				agent_failures: [
					{
						...scoringFailure,
						attempts: 5,
						failed: true,
						next_attempt_at: '2026-02-01T06:00:00Z',
					},
				],
			},
		})
		const notice = container.querySelector('[data-agent-failure="scoring"]')
		expect(notice?.getAttribute('data-failed')).toBe('true')
		expect(
			notice?.querySelector('[data-part="failure-status"]')?.textContent,
		).toBe('En échec après 5 tentatives')
		expect(
			container.querySelector('[data-part="combined"] [data-verdict-reason]')
				?.textContent,
		).toBe('En attente : appel au modèle en échec après 5 tentatives')
		fireEvent.click(screen.getByRole('button', { name: 'Réessayer' }))
		expect(onRetryAgents).toHaveBeenCalled()
	})

	it('disables the agents retry while it runs and shows its error', () => {
		renderDetail({
			retryingAgents: true,
			retryAgentsError: 'refusé',
			detail: {
				...detail,
				agent_failures: [{ ...scoringFailure, attempts: 5, failed: true }],
			},
		})
		expect(
			(screen.getByRole('button', { name: 'Réessayer' }) as HTMLButtonElement)
				.disabled,
		).toBe(true)
		expect(screen.getByRole('alert').textContent).toBe('refusé')
	})

	it('shows a qualification call failure in the qualification section only', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				qualification_results: [],
				agent_failures: [
					{
						...scoringFailure,
						kind: 'qualification',
						agent_id: 'pertinence',
						agent_name: 'Pertinence',
					},
				],
			},
		})
		expect(
			container.querySelector(
				'[data-section="agents"] [data-agent-failure="qualification"]',
			)?.textContent,
		).toContain('Agent « Pertinence »')
		expect(
			container.querySelector('[data-section="scoring"] [data-agent-failure]'),
		).toBeNull()
	})

	it('says which agents stopped on this lead and why', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				agent_stops: [
					{
						kind: 'qualification',
						agent_id: 'marche',
						agent_name: 'Marché',
						reason: 'agent_removed',
						profile_revision: 2,
						stopped_at: '2026-02-01T00:00:00Z',
					},
					{
						kind: 'scoring',
						agent_id: 'score-global',
						agent_name: 'Score global',
						reason: 'profile_deactivated',
						profile_revision: 2,
						stopped_at: '2026-02-01T00:00:00Z',
					},
				],
			},
		})
		const stops = container.querySelectorAll('[data-agent-stop]')
		expect(stops).toHaveLength(2)
		expect(stops[0].getAttribute('data-agent-stop')).toBe('qualification')
		expect(stops[0].textContent).toContain('Agent « Marché »')
		expect(stops[0].textContent).toContain('Arrêté : agent supprimé')
		expect(stops[1].textContent).toContain('Arrêté : profil désactivé')
		expect(stops[1].textContent).toContain(
			formatDateTime('2026-02-01T00:00:00Z'),
		)
	})

	it('shows no stop notice when every agent ran', () => {
		const { container } = renderDetail()
		expect(container.querySelector('[data-section="agent-stops"]')).toBeNull()
	})

	it('keeps the usual wording when no call failed', () => {
		const { container } = renderDetail({
			detail: { ...detail, lead: pendingScoring },
		})
		expect(container.querySelector('[data-agent-failure]')).toBeNull()
		expect(
			container.querySelector('[data-part="combined"] [data-verdict-reason]')
				?.textContent,
		).toBe("En attente : aucun agent de scoring n'a répondu")
	})

	it('shows the facts block with its environments, sources, dates and country', () => {
		const { container } = renderDetail()
		const facts = container.querySelector('[data-section="facts"]')
		expect(facts?.textContent).toContain('Faits')
		expect(facts?.textContent).toContain('1 hôte · dev')
		expect(facts?.textContent).toContain('Sous-domaines vus3')
		expect(facts?.textContent).toContain('Signaux1')
		expect(facts?.textContent).toContain('Sources1 · crawler')
		expect(facts?.textContent).toContain('Première détection')
		expect(facts?.textContent).toContain('Dernière détection')
		expect(facts?.textContent).toContain('Pays probableFrance')
	})

	it('shows the enrichment steps next to the facts', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				enrichment: {
					status: 'pending',
					steps: [
						{
							step: 'http_probe',
							enabled: true,
							ran_at: '2026-02-01T10:00:00Z',
						},
						{ step: 'company_jobs', enabled: false, ran_at: null },
					],
					waiting_reason: 'awaiting_enrichment',
					wait_until: '2026-02-01T10:30:00Z',
				},
			},
		})
		const facts = container.querySelector('[data-section="facts"]')
		const block = container.querySelector('[data-section="enrichment"]')
		expect(block).toBeTruthy()
		expect(
			facts && block
				? facts.compareDocumentPosition(block) &
						Node.DOCUMENT_POSITION_FOLLOWING
				: 0,
		).toBeTruthy()
		const probe = block?.querySelector('[data-step="http_probe"]')
		expect(probe?.getAttribute('data-state')).toBe('done')
		expect(probe?.textContent).toContain('Sonde HTTP')
		expect(probe?.textContent).toContain('Fait')
		expect(probe?.textContent).toContain(formatDateTime('2026-02-01T10:00:00Z'))
		const jobs = block?.querySelector('[data-step="company_jobs"]')
		expect(jobs?.getAttribute('data-state')).toBe('disabled')
		expect(jobs?.textContent).toContain("Offres d'emploi de l'entreprise")
		expect(jobs?.textContent).toContain('Désactivé')
		const waiting = block?.querySelector('[data-waiting]')
		expect(waiting?.textContent).toBe(
			`En attente d'enrichissement jusqu'au ${formatDateTime('2026-02-01T10:30:00Z')}`,
		)
	})

	it('flags a certificate-only lead waiting for other evidence', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: pendingScoring,
				enrichment: {
					status: 'complete',
					steps: [
						{
							step: 'http_probe',
							enabled: true,
							ran_at: '2026-02-01T10:00:00Z',
						},
					],
					waiting_reason: 'awaiting_enrichment_evidence',
					wait_until: null,
				},
			},
		})
		expect(
			container.querySelector('[data-section="enrichment"] [data-waiting]')
				?.textContent,
		).toBe("En attente de preuve d'enrichissement")
		expect(container.querySelector('[data-verdict]')?.textContent).toBe(
			"En attente de preuve d'enrichissement",
		)
	})

	it('shows the wait in the header verdict while the lead has no verdict', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: pendingScoring,
				enrichment: {
					status: 'pending',
					steps: [{ step: 'http_probe', enabled: true, ran_at: null }],
					waiting_reason: 'awaiting_enrichment',
					wait_until: null,
				},
			},
		})
		expect(container.querySelector('[data-verdict]')?.textContent).toBe(
			"En attente d'enrichissement",
		)
	})

	it('says so when the profile enables no enrichment step', () => {
		const { container } = renderDetail()
		const block = container.querySelector('[data-section="enrichment"]')
		expect(block?.textContent).toContain('Aucune étape activée')
		expect(block?.querySelector('[data-waiting]')).toBeNull()
	})

	it('names the host of a certificate signal', () => {
		renderDetail({
			detail: {
				...detail,
				signals: [
					{
						id: 'c1',
						kind: 'certificate_seen',
						value: 'sso.acme.test',
						sentence:
							'sso.acme.test apparaît dans les certificats publics (Certificate Transparency)',
						source: 'certificate_transparency',
						agent: 'certificats',
						provenance: 'https://crt.sh/?q=sso.acme.test',
						strength: 0.3,
						observed_at: '2026-02-01T00:00:00Z',
						raw: {},
					},
				],
			},
		})
		expect(
			screen.getByText('Certificat observé pour sso.acme.test'),
		).toBeTruthy()
	})

	it('warns when no signal comes from a production host', () => {
		const { container } = renderDetail()
		expect(
			container.querySelector('[data-warning="non-production"]')?.textContent,
		).toBe('Uniquement des hôtes hors production')
	})

	it('shows no warning when a production host exists', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				facts: {
					...detail.facts,
					hosts: [
						{ host: 'sso.acme.test', environment: 'production', signals: 1 },
					],
					production_hosts: 1,
					production_share: 1,
					only_non_production: false,
				},
			},
		})
		expect(container.querySelector('[data-warning]')).toBeNull()
		expect(
			container.querySelector('[data-section="facts"]')?.textContent,
		).toContain('1 hôte · prod')
	})

	it('shows an unknown country and no host for a lead without any', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				facts: {
					...detail.facts,
					hosts: [],
					distinct_hosts: 0,
					production_hosts: 0,
					production_share: null,
					only_non_production: false,
					probable_country: null,
					sources: [],
					source_count: 0,
					signal_count: 0,
				},
			},
		})
		const facts = container.querySelector('[data-section="facts"]')
		expect(facts?.textContent).toContain('Aucun hôte identifié')
		expect(facts?.textContent).toContain('Pays probableInconnu')
		expect(facts?.textContent).toContain('SourcesAucune')
		expect(container.querySelector('[data-warning]')).toBeNull()
	})

	it('renders without facts or signal sentences from an older API', () => {
		const legacy = {
			...detail,
			facts: undefined,
			signals: detail.signals.map((signal) => ({
				...signal,
				sentence: undefined,
			})),
		} as unknown as LeadDetailData
		const { container } = renderDetail({ detail: legacy })
		expect(container.querySelector('[data-section="facts"]')).toBeNull()
		const first = detail.signals[0]
		expect(container.textContent).toContain(`${first.kind} : ${first.value}`)
	})

	it('lists the signals as readable sentences', () => {
		renderDetail()
		expect(
			screen.getByText('acme.test publie une page de recrutement (careers)'),
		).toBeTruthy()
	})

	it('renders header, signals, contacts and assessments without a score breakdown', () => {
		renderDetail()
		expect(screen.getByRole('heading', { name: 'acme.test' })).toBeTruthy()
		expect(screen.getByText('Fintech')).toBeTruthy()
		expect(screen.queryByText('Détail du score')).toBeNull()
		expect(screen.getByText('Source : crawler')).toBeTruthy()
		expect(screen.getByText('ats-fr')).toBeTruthy()
		expect(screen.getByText('jane@acme.test')).toBeTruthy()
		expect(screen.getByText('Growing team')).toBeTruthy()
		expect(screen.getByText('Modèle : model-x')).toBeTruthy()
		expect(screen.getByText('Prompt : v2')).toBeTruthy()
		expect(screen.getByText('Révision du profil : 3')).toBeTruthy()
		expect(screen.getByText('Hello')).toBeTruthy()
		expect(screen.getByText('En attente')).toBeTruthy()
	})

	it('shows the verdict, its confidence and the score out of 100 in the header', () => {
		const { container } = renderDetail()
		const badge = container.querySelector('h1 ~ [data-verdict]')
		expect(badge?.getAttribute('data-verdict')).toBe('a_verifier')
		expect(badge?.textContent).toBe('À vérifier')
		expect(screen.getAllByText('Confiance du verdict').length).toBeGreaterThan(
			0,
		)
		expect(
			container.querySelector('[data-part="verdict-confidence"]')?.textContent,
		).toMatch(/^72,5\s%$/)
		expect(
			container.querySelector('dl [data-part="llm-score"]')?.textContent,
		).toBe('68 / 100')
	})

	it('names the signal strength apart from the score of the scoring agents', () => {
		renderDetail()
		expect(screen.queryByText('Score des signaux')).toBeNull()
		expect(
			screen.getByText('Force des signaux', { selector: 'dt' }),
		).toBeTruthy()
		expect(
			screen.getByText('Force des signaux', { selector: 'dt' }).nextSibling
				?.textContent,
		).toBe('42,5 / 100')
	})

	it('puts the indicators, opinion by evidence, in the calculation', () => {
		const { container } = renderDetail()
		const indicators = container.querySelector(
			'[data-section="confidence-calculation"] [data-part="indicators"]',
		)
		expect(indicators?.textContent).toContain('Indicateurs')
		expect(indicators?.textContent).toContain('ne décident pas du verdict')
		expect(
			indicators?.querySelector('[data-indicator="opinion"]')?.textContent,
		).toMatch(/Avis des agents92,9\s%/)
		expect(
			indicators?.querySelector('[data-indicator="evidence"]')?.textContent,
		).toMatch(/Solidité des preuves35\s%/)
		expect(
			indicators?.querySelector('[data-indicator="confidence"]')?.textContent,
		).toMatch(/Confiance des indicateurs41,5\s%/)
	})

	it('lists the evidence components and the indicator cap in the calculation', () => {
		const { container } = renderDetail()
		const calculation = container.querySelector(
			'[data-section="confidence-calculation"]',
		)
		expect(calculation?.textContent).toContain('Calcul')
		for (const label of [
			'Volume de signaux',
			'Diversité des sources',
			'Environnement',
			'Force des signaux',
		]) {
			expect(calculation?.textContent).toContain(label)
		}
		expect(
			calculation?.querySelector('[data-evidence-component="volume"]')
				?.textContent,
		).toMatch(/28,4\s%/)
		expect(
			calculation?.querySelector('[data-indicator-cap]')?.textContent,
		).toBe('Indicateurs plafonnés : uniquement des hôtes hors production')
	})

	it('opens the calculation on a pending verdict and says why', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: {
					...detail.lead,
					verdict: null,
					confidence: null,
					llm_score: null,
					verdict_probabilities: null,
					verdict_reason: 'awaiting_scoring',
				},
			},
		})
		const calculation = container.querySelector(
			'[data-section="confidence-calculation"]',
		)
		expect(calculation?.hasAttribute('open')).toBe(true)
		expect(
			calculation?.querySelector('[data-verdict-reason]')?.textContent,
		).toBe("En attente : aucun agent de scoring n'a répondu")
		expect(container.querySelector('h1 ~ [data-verdict]')?.textContent).toBe(
			'En attente du scoring',
		)
		expect(
			container.querySelector('dl [data-part="llm-score"]')?.textContent,
		).toBe('—')
	})

	it('keeps the former cap wording on a lead still carrying an indicator verdict', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: {
					...detail.lead,
					verdict_source: 'indicators',
					verdict_reason: 'non_production',
				},
			},
		})
		expect(container.querySelector('[data-verdict-reason]')?.textContent).toBe(
			'Plafonné à « À vérifier » : uniquement des hôtes hors production',
		)
		expect(container.querySelector('[data-indicator-cap]')).toBeNull()
	})

	it('shows no calculation before the lead was qualified', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: {
					...detail.lead,
					evidence: null,
					opinion: null,
					indicator_confidence: null,
					indicator_cap: null,
					verdict_reason: null,
				},
			},
		})
		expect(
			container.querySelector('[data-section="confidence-calculation"]'),
		).toBeNull()
		expect(screen.queryByText('Avis des agents')).toBeNull()
	})

	it('puts the summary on top with verdict narrative, angle and evidence', () => {
		const { container } = renderDetail()
		const summary = container.querySelector('[data-section="summary"]')
		expect(summary?.textContent).toContain('Synthèse')
		expect(summary?.textContent).toContain('Growing team')
		expect(summary?.textContent).toContain(
			'Verdict : Both agents agree the lead is relevant.',
		)
		expect(summary?.textContent).toContain("Angle d'approche : Offer tooling")
		expect(summary?.textContent).toContain('We are hiring')
		expect(summary?.querySelector('a')?.getAttribute('href')).toBe(
			'https://acme.test/careers',
		)
		expect(screen.queryByText(/Preuves non vérifiées/)).toBeNull()
		const sections = Array.from(
			container.querySelectorAll('[data-slot="card"]'),
		)
		const position = (node: Element | null) =>
			node
				? sections.indexOf(node.closest('[data-slot="card"]') as Element)
				: -1
		expect(
			position(container.querySelector('article[data-agent]')),
		).toBeLessThan(position(summary))
		expect(summary?.textContent).toContain(
			'Synthèse finale, rédigée par le modèle à partir des rapports de tous les agents',
		)
	})

	it('warns when the evidence could not be verified', () => {
		const first = detail.assessments[0]
		if (!first) throw new Error('fixture without assessment')
		renderDetail({
			detail: {
				...detail,
				assessments: [{ ...first, evidence_verified: false, evidence: [] }],
			},
		})
		expect(screen.getByRole('status').textContent).toContain(
			'Preuves non vérifiées',
		)
		expect(screen.getByText('Growing team')).toBeTruthy()
	})

	it('warns in French when the synthesis is of uncertain quality', () => {
		const first = detail.assessments[0]
		if (!first) throw new Error('fixture without assessment')
		renderDetail({
			detail: {
				...detail,
				assessments: [
					{ ...first, quality_warnings: ['repetition', 'restated_figures'] },
				],
			},
		})
		expect(screen.getByRole('alert').textContent).toBe(
			'Synthèse de qualité incertaine : répétitions, pourcentage ou verdict répétés avec un chiffre',
		)
		expect(screen.queryByRole('status')).toBeNull()
		expect(screen.getByText('Growing team')).toBeTruthy()
	})

	it('shows no quality warning for a clean synthesis or an older API', () => {
		const first = detail.assessments[0]
		if (!first) throw new Error('fixture without assessment')
		const { quality_warnings: _omitted, ...older } = first
		renderDetail({
			detail: { ...detail, assessments: [older as typeof first] },
		})
		expect(screen.queryByText(/qualité incertaine/)).toBeNull()
		expect(screen.getByText('Growing team')).toBeTruthy()
	})

	it('links the summary to its draft in the validation queue', () => {
		const onOpenDraft = vi.fn()
		renderDetail({ onOpenDraft })
		fireEvent.click(
			screen.getByRole('button', { name: 'Ouvrir dans la file de validation' }),
		)
		expect(onOpenDraft).toHaveBeenCalledWith('a1')
	})

	it('shows a failed summary with its reason and a retry button', () => {
		const onRetrySummary = vi.fn()
		const { container } = renderDetail({
			onRetrySummary,
			detail: {
				...detail,
				assessments: [],
				summary_state: {
					attempts: 5,
					failed: true,
					error: "échec de l'appel au modèle",
					next_attempt_at: '2026-02-01T06:00:00Z',
				},
			},
		})
		expect(
			container.querySelector('[data-summary-failed="true"]')?.textContent,
		).toContain("Bilan en échec : échec de l'appel au modèle")
		fireEvent.click(screen.getByRole('button', { name: 'Réessayer' }))
		expect(onRetrySummary).toHaveBeenCalled()
	})

	it('shows an ongoing backoff without the failed wording', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				summary_state: {
					attempts: 2,
					failed: false,
					error: 'réponse non conforme au contrat',
					next_attempt_at: '2026-02-01T00:05:00Z',
				},
			},
		})
		const box = container.querySelector('[data-summary-failed="false"]')
		expect(box?.textContent).toContain('Le bilan a échoué 2 fois')
		expect(box?.textContent).not.toContain('Bilan en échec')
	})

	it('disables the retry while it runs and shows its error', () => {
		renderDetail({
			retryingSummary: true,
			retrySummaryError: 'refusé',
			detail: {
				...detail,
				summary_state: {
					attempts: 5,
					failed: true,
					error: 'x',
					next_attempt_at: null,
				},
			},
		})
		expect(
			(screen.getByRole('button', { name: 'Réessayer' }) as HTMLButtonElement)
				.disabled,
		).toBe(true)
		expect(screen.getByRole('alert').textContent).toBe('refusé')
	})

	it('shows no failure box for a healthy lead', () => {
		const { container } = renderDetail()
		expect(container.querySelector('[data-summary-failed]')).toBeNull()
	})

	it('says so when the lead has no summary yet', () => {
		renderDetail({ detail: { ...detail, assessments: [] } })
		expect(
			screen.getByText(/Pas encore de synthèse : elle est rédigée/),
		).toBeTruthy()
		expect(screen.queryByText(/Synthèses précédentes/)).toBeNull()
	})

	it('keeps older summaries in a history below', () => {
		const first = detail.assessments[0]
		if (!first) throw new Error('fixture without assessment')
		renderDetail({
			detail: {
				...detail,
				assessments: [
					first,
					{ ...first, id: 'a0', summary: 'Older summary', draft: null },
				],
			},
		})
		expect(screen.getByText('Synthèses précédentes (1)')).toBeTruthy()
		expect(screen.getByText('Older summary')).toBeTruthy()
		expect(screen.getAllByText('Growing team')).toHaveLength(1)
	})

	it('renders one report card per agent in profile order with its parts', () => {
		const { container } = renderDetail()
		expect(screen.getByText('Analyse par agent (2)')).toBeTruthy()
		const cards = container.querySelectorAll('article[data-agent]')
		expect(
			Array.from(cards).map((card) => card.getAttribute('data-agent')),
		).toEqual(['pertinence', 'maturite'])
		const valid = container.querySelector('article[data-agent="pertinence"]')
		expect(valid?.getAttribute('data-status')).toBe('valid')
		expect(valid?.textContent).toContain('Pertinence')
		expect(valid?.textContent).toContain('Valide')
		expect(valid?.textContent).toContain('Laya (gemma_laya)')
		expect(valid?.textContent).toMatch(/Score de l'agent : 62,5\s%/)
		expect(valid?.querySelector('[data-part="evaluates"]')?.textContent).toBe(
			"Ce qu'il évalue : La pertinence du lead pour l'offre.",
		)
		expect(valid?.querySelector('[data-part="summary"]')?.textContent).toBe(
			'Le lead exploite un Keycloak ancien et recrute.',
		)
		const text = valid?.textContent ?? ''
		expect(text.indexOf("Ce qu'il évalue")).toBeLessThan(
			text.indexOf('Le lead exploite un Keycloak'),
		)
		expect(text.indexOf('Le lead exploite un Keycloak')).toBeLessThan(
			text.indexOf('Mesures'),
		)
		const measure = valid?.querySelector('[data-question="pertinence"]')
		expect(measure?.textContent).toContain(
			"Ce lead est-il pertinent pour l'offre ?",
		)
		expect(measure?.querySelector('code')?.textContent).toBe('pertinence')
		expect(measure?.textContent).toContain('Réponse : Moyenne')
		expect(measure?.textContent).toMatch(/confiance 80\s%/)
		const bars = valid?.querySelectorAll('[data-probability]') ?? []
		expect(Array.from(bars).map((bar) => bar.getAttribute('style'))).toEqual([
			'width: 25%;',
			'width: 50%;',
			'width: 25%;',
		])
	})

	it('renders the scoring section between the agents analysis and the summary', () => {
		const { container } = renderDetail({
			detail: { ...detail, scoring_results: [scoringResult] },
		})
		const sections = Array.from(
			container.querySelectorAll('[data-section]'),
		).map((node) => node.getAttribute('data-section'))
		const agents = sections.indexOf('agents')
		const scoring = sections.indexOf('scoring')
		const summary = sections.indexOf('summary')
		expect(agents).toBeGreaterThanOrEqual(0)
		expect(scoring).toBeGreaterThan(agents)
		expect(summary).toBeGreaterThan(scoring)
		expect(screen.getByText('Scoring (1)')).toBeTruthy()
	})

	it('shows one card per scoring agent with its score, verdict, bars and reasons', () => {
		const { container } = renderDetail({
			detail: { ...detail, scoring_results: [scoringResult] },
		})
		const card = container.querySelector(
			'article[data-scoring-agent="score-global"]',
		)
		expect(card?.getAttribute('data-status')).toBe('valid')
		expect(card?.textContent).toContain('Score global')
		expect(card?.textContent).toContain('Valide')
		expect(card?.textContent).not.toContain('poids')
		expect(card?.querySelector('[data-part="agent-score"]')?.textContent).toBe(
			'72 / 100',
		)
		expect(card?.querySelector('[data-part="model"]')?.textContent).toContain(
			'Synthèse (gemma_laya)',
		)
		expect(card?.querySelector('[data-part="evaluates"]')?.textContent).toBe(
			"Ce qu'il évalue : Décide du score et du verdict.",
		)
		expect(
			card?.querySelector('[data-verdict]')?.getAttribute('data-verdict'),
		).toBe('a_verifier')
		const bars = card?.querySelectorAll('[data-verdict-probability]') ?? []
		expect(
			Array.from(bars).map((bar) => [
				bar.getAttribute('data-verdict-probability'),
				bar.getAttribute('style'),
			]),
		).toEqual([
			['fiable', 'width: 30%;'],
			['a_verifier', 'width: 60%;'],
			['ecarte', 'width: 10%;'],
		])
		expect(card?.textContent).toMatch(/probabilité 60\s%/)
		expect(
			Array.from(card?.querySelectorAll('[data-part="reasons"] li') ?? []).map(
				(item) => item.textContent,
			),
		).toEqual(['Un seul signal.', 'Entreprise ciblée.'])
		expect(
			card?.querySelector('[data-part="declared"]')?.textContent,
		).toContain('Verdict annoncé par le modèle : Fiable')
	})

	it('shows the raw and the calibrated verdict probabilities of a calibrated agent', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				scoring_results: [
					{
						...scoringResult,
						calibration: {
							n: 80,
							probabilities: { fiable: 10, a_verifier: 60, ecarte: 30 },
						},
					},
				],
			},
		})
		const card = container.querySelector(
			'article[data-scoring-agent="score-global"]',
		)
		expect(
			card?.querySelector('[data-part="calibration-badge"]')?.textContent,
		).toBe('Calibré (n = 80)')
		const widths = (part: string) =>
			Array.from(
				card?.querySelectorAll(
					`[data-part="${part}"] [data-verdict-probability]`,
				) ?? [],
			).map((bar) => bar.getAttribute('style'))
		expect(widths('verdict-probabilities-raw')).toEqual([
			'width: 30%;',
			'width: 60%;',
			'width: 10%;',
		])
		expect(widths('verdict-probabilities-calibrated')).toEqual([
			'width: 10%;',
			'width: 60%;',
			'width: 30%;',
		])
		expect(
			card?.querySelector('[data-part="verdict-probabilities"]'),
		).toBeNull()
		expect(card?.textContent).toContain('Probabilités brutes de l')
		expect(card?.textContent).toContain('Probabilités calibrées')
	})

	it('shows no calibration mark on an agent that is not calibrated', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				scoring_results: [{ ...scoringResult, calibration: null }],
			},
		})
		const card = container.querySelector(
			'article[data-scoring-agent="score-global"]',
		)
		expect(card?.querySelector('[data-part="calibration"]')).toBeNull()
		expect(card?.textContent).not.toContain('Calibré')
		expect(
			card?.querySelectorAll('[data-part="verdict-probabilities"]'),
		).toHaveLength(1)
	})

	it('shows the criteria with their quotes and flags the ungrounded ones', () => {
		const { container } = renderDetail({
			detail: { ...detail, scoring_results: [scoringResult] },
		})
		const card = container.querySelector(
			'article[data-scoring-agent="score-global"]',
		)
		const grounded = card?.querySelector('[data-question="adequation_offre"]')
		expect(grounded?.textContent).toContain('Réponse : Forte')
		expect(
			grounded?.querySelector('[data-part="evidence"]')?.textContent,
		).toContain('« expose Keycloak 18 »')
		const ungrounded = card?.querySelector(
			'[data-question="solidite_preuves"] [data-part="ungrounded"]',
		)
		expect(ungrounded?.textContent).toContain(
			'Non étayé — ramené à « Inconnu »',
		)
		expect(ungrounded?.textContent).toContain(
			'Choix initial du modèle : Solide',
		)
	})

	it('shows an invalid scoring agent with its error and no verdict', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				scoring_results: [
					{
						...scoringResult,
						status: 'invalid',
						score: null,
						verdict: null,
						reasons: [],
						criteria: [],
						error:
							'Réponse non conforme au contrat : `score` doit être un entier',
					},
				],
			},
		})
		const card = container.querySelector('article[data-scoring-agent]')
		expect(card?.getAttribute('data-status')).toBe('invalid')
		expect(card?.textContent).toContain('Invalide')
		expect(card?.textContent).toContain('Réponse non conforme au contrat')
		expect(card?.querySelector('[data-part="verdict"]')).toBeNull()
		expect(card?.querySelector('[data-part="agent-score"]')).toBeNull()
	})

	it('shows the combined result under the agent cards', () => {
		const { container } = renderDetail({
			detail: { ...detail, scoring_results: [scoringResult] },
		})
		const combined = container.querySelector('[data-part="combined"]')
		expect(
			combined?.querySelector('[data-part="combined-score"]')?.textContent,
		).toBe('68 / 100')
		expect(
			combined?.querySelector('[data-verdict]')?.getAttribute('data-verdict'),
		).toBe('a_verifier')
		expect(
			combined?.querySelector('[data-part="combined-confidence"]')?.textContent,
		).toMatch(/72,5\s%/)
		expect(
			Array.from(
				combined?.querySelectorAll('[data-verdict-probability]') ?? [],
			).map((bar) => bar.getAttribute('style')),
		).toEqual(['width: 20%;', 'width: 73%;', 'width: 8%;'])
	})

	it('says so when no scoring agent answered and shows the pending combination', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: {
					...detail.lead,
					verdict: null,
					confidence: null,
					llm_score: null,
					verdict_probabilities: null,
					verdict_reason: 'awaiting_scoring',
				},
			},
		})
		const section = container.querySelector('[data-section="scoring"]')
		expect(section?.textContent).toContain(
			"Aucun agent de scoring n'a répondu pour le moment.",
		)
		const combined = section?.querySelector('[data-part="combined"]')
		expect(
			combined?.querySelector('[data-part="combined-score"]')?.textContent,
		).toBe('—')
		expect(combined?.querySelector('[data-verdict]')?.textContent).toBe(
			'En attente du scoring',
		)
		expect(combined?.querySelector('[data-verdict-reason]')?.textContent).toBe(
			"En attente : aucun agent de scoring n'a répondu",
		)
		expect(combined?.querySelector('[data-verdict-probability]')).toBeNull()
	})

	it('explains an exclusion in the combined result', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: { ...detail.lead, verdict: 'ecarte', verdict_reason: 'excluded' },
			},
		})
		expect(
			container.querySelector('[data-part="combined"] [data-verdict-reason]')
				?.textContent,
		).toBe('Écarté : un agent de qualification a exclu le lead')
	})

	it('shows the verbatim quote in italics with its source under the answer', () => {
		const { container } = renderDetail()
		const evidence = container.querySelector(
			'article[data-agent="pertinence"] [data-part="evidence"]',
		)
		expect(evidence?.textContent).toContain(
			'« expose Keycloak 18 en production »',
		)
		expect(evidence?.querySelector('p.italic')).not.toBeNull()
		const link = evidence?.querySelector('a')
		expect(link?.getAttribute('href')).toBe('https://acme.test/realms/master')
		expect(container.querySelector('[data-part="ungrounded"]')).toBeNull()
	})

	it('names the computed facts as the source of a quote taken from them', () => {
		const first = detail.qualification_results[0]
		const answer = first?.answers[0]
		if (!first || !answer) throw new Error('fixture without answer')
		const { container } = renderDetail({
			detail: {
				...detail,
				qualification_results: [
					{
						...first,
						answers: [{ ...answer, evidence_source: 'lead' }],
					},
				],
			},
		})
		const source = container.querySelector('[data-part="evidence-source"]')
		expect(source?.textContent).toContain('Faits calculés par le système')
		expect(source?.querySelector('a')).toBeNull()
	})

	it('omits the quote block when the answer has no quote', () => {
		const first = detail.qualification_results[0]
		const answer = first?.answers[0]
		if (!first || !answer) throw new Error('fixture without answer')
		const { container } = renderDetail({
			detail: {
				...detail,
				qualification_results: [
					{
						...first,
						answers: [{ ...answer, evidence: '', evidence_source: null }],
					},
				],
			},
		})
		expect(container.querySelector('[data-part="evidence"]')).toBeNull()
		expect(container.querySelector('[data-part="ungrounded"]')).toBeNull()
	})

	it('flags a downgraded answer with the original choice and hides its quote', () => {
		const first = detail.qualification_results[0]
		const answer = first?.answers[0]
		if (!first || !answer) throw new Error('fixture without answer')
		const { container } = renderDetail({
			detail: {
				...detail,
				qualification_results: [
					{
						...first,
						answers: [
							{
								...answer,
								choice: 'inconnu',
								choice_label: 'Inconnu',
								grounded: false,
								original_choice: 'forte',
								original_choice_label: 'Forte',
								evidence: 'phrase inventée',
								evidence_source: null,
							},
						],
					},
				],
			},
		})
		const flag = container.querySelector('[data-part="ungrounded"]')
		expect(flag?.textContent).toContain('Non étayé — ramené à « Inconnu »')
		expect(flag?.textContent).toContain('Choix initial du modèle : Forte')
		expect(container.querySelector('[data-part="evidence"]')).toBeNull()
		expect(container.textContent).not.toContain('phrase inventée')
	})

	it('flags a valid result stored before agent summaries', () => {
		const first = detail.qualification_results[0]
		if (!first) throw new Error('fixture without result')
		const { container } = renderDetail({
			detail: {
				...detail,
				qualification_results: [
					{ ...first, summary: null, agent_description: null },
				],
			},
		})
		expect(container.querySelector('[data-part="summary"]')).toBeNull()
		expect(container.querySelector('[data-part="evaluates"]')).toBeNull()
		expect(container.querySelector('[data-part="no-summary"]')).not.toBeNull()
	})

	it('shows the error of an invalid agent result without a score', () => {
		const { container } = renderDetail()
		const invalid = container.querySelector('article[data-agent="maturite"]')
		expect(invalid?.getAttribute('data-status')).toBe('invalid')
		expect(invalid?.textContent).toContain('Invalide')
		expect(invalid?.textContent).toContain('fournisseur indisponible')
		expect(invalid?.textContent).toContain(
			"Ce qu'il évalue : La maturité technique du lead.",
		)
		expect(invalid?.textContent).not.toContain("Score de l'agent")
		expect(invalid?.querySelector('[data-part="measures"]')).toBeNull()
		expect(invalid?.querySelectorAll('[data-probability]')).toHaveLength(0)
	})

	it('says so when no agent has answered yet', () => {
		renderDetail({
			detail: {
				...detail,
				lead: { ...detail.lead, verdict: null, confidence: null },
				qualification_results: [],
			},
		})
		expect(
			screen.getByText('Aucune qualification pour le moment.'),
		).toBeTruthy()
		expect(screen.getByText('Analyse par agent (0)')).toBeTruthy()
	})

	it('renders evidence as a blockquote with an external provenance link', () => {
		const { container } = renderDetail()
		const quote = container.querySelector('blockquote')
		expect(quote?.textContent).toContain('We are hiring')
		const link = quote?.querySelector('a')
		expect(link?.getAttribute('href')).toBe('https://acme.test/careers')
		expect(link?.getAttribute('target')).toBe('_blank')
		expect(link?.getAttribute('rel')).toBe('noopener noreferrer')
	})

	it('only offers the allowed next statuses', () => {
		renderDetail({ nextStatuses: ['validated', 'rejected', 'new'] })
		expect(screen.getByRole('button', { name: 'Validé' })).toBeTruthy()
		expect(screen.queryByRole('button', { name: 'Contacté' })).toBeNull()
	})

	it('reports the chosen status', () => {
		const onChangeStatus = vi.fn()
		renderDetail({ onChangeStatus })
		fireEvent.click(screen.getByRole('button', { name: 'Rejeté' }))
		expect(onChangeStatus).toHaveBeenCalledWith('rejected')
	})

	it('shows the server message when a transition is refused', () => {
		renderDetail({ changeError: 'transition not allowed' })
		expect(screen.getByRole('alert').textContent).toBe('transition not allowed')
	})

	const completeCompany: NonNullable<LeadDetailData['company']> = {
		state: 'complete',
		siren: '552081317',
		official: {
			siren: '552081317',
			legal_name: 'ELECTRICITE DE FRANCE',
			naf_code: '35.11Z',
			naf_label: "Production et distribution d'électricité",
			headcount_range: '10 000 salariés et plus',
			head_office_city: 'PARIS',
			head_office_postcode: '75008',
			head_office_country: 'France',
			created_on: '1955-01-01',
			annuaire_url:
				'https://annuaire-entreprises.data.gouv.fr/entreprise/552081317',
		},
		pages: [
			{ url: 'https://acme.test/' },
			{ url: 'https://acme.test/mentions-legales' },
		],
		ignored_links: 0,
		built_at: '2026-10-06T10:00:00Z',
		next_attempt_at: '2027-01-04T10:00:00Z',
		error: null,
		siblings: [],
	}

	it('shows the company block with its official data, pages and state', () => {
		const { container } = renderDetail({
			detail: { ...detail, company: completeCompany },
		})
		const card = container.querySelector('[data-section="company"]')
		expect(card?.textContent).toContain('Fiche entreprise')
		expect(
			card
				?.querySelector('[data-company-state]')
				?.getAttribute('data-company-state'),
		).toBe('complete')
		const text = card?.textContent ?? ''
		expect(text).toContain('Raison socialeELECTRICITE DE FRANCE')
		expect(text).toContain('SIREN552 081 317')
		expect(text).toContain(
			"Activité (NAF)35.11Z · Production et distribution d'électricité",
		)
		expect(text).toContain('Effectif10 000 salariés et plus')
		expect(text).toContain('Siège75008 PARIS, France')
		expect(text).toContain('Création1 janvier 1955')
		const siren = card?.querySelector('dl a')
		expect(siren?.getAttribute('href')).toBe(
			'https://annuaire-entreprises.data.gouv.fr/entreprise/552081317',
		)
		const pages = card?.querySelectorAll('[data-company-pages] a') ?? []
		expect([...pages].map((link) => link.getAttribute('href'))).toEqual([
			'https://acme.test/',
			'https://acme.test/mentions-legales',
		])
		expect(text).toContain('Dernière mise à jour')
	})

	it('lists only the confirmed pages and counts the ignored links', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				company: {
					...completeCompany,
					pages: [{ url: 'https://acme.test/' }],
					ignored_links: 4,
				},
			},
		})
		const card = container.querySelector('[data-section="company"]')
		const pages = card?.querySelectorAll('[data-company-pages] a') ?? []
		expect([...pages].map((link) => link.getAttribute('href'))).toEqual([
			'https://acme.test/',
		])
		expect(card?.querySelector('[data-company-ignored]')?.textContent).toBe(
			'4 liens ignorés : pages introuvables, vides ou redirigées vers l’accueil.',
		)
		expect(card?.querySelector('[data-company-no-pages]')).toBeNull()
	})

	it('says that no usable page was found when none was confirmed', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				company: { ...completeCompany, pages: [], ignored_links: 2 },
			},
		})
		const card = container.querySelector('[data-section="company"]')
		expect(card?.querySelector('[data-company-pages]')).toBeNull()
		expect(card?.querySelector('[data-company-no-pages]')?.textContent).toBe(
			'Aucune page exploitable trouvée',
		)
		expect(card?.textContent).toContain('Pages lues')
		expect(card?.querySelector('[data-company-ignored]')?.textContent).toBe(
			'2 liens ignorés : pages introuvables, vides ou redirigées vers l’accueil.',
		)
	})

	it('does not announce missing pages while the dossier is pending', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				company: { ...completeCompany, state: 'pending', pages: [] },
			},
		})
		const card = container.querySelector('[data-section="company"]')
		expect(card?.querySelector('[data-company-no-pages]')).toBeNull()
		expect(card?.querySelector('[data-company-ignored]')).toBeNull()
	})

	it('shows an explicit notice when the SIREN was not found', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				company: {
					...completeCompany,
					state: 'partial',
					siren: null,
					official: null,
				},
			},
		})
		const card = container.querySelector('[data-section="company"]')
		expect(card?.querySelector('[data-company-official]')).toBeNull()
		expect(card?.querySelector('[data-company-notice]')?.textContent).toContain(
			'SIREN introuvable',
		)
		expect(card?.textContent).toContain('Partielle')
	})

	it('shows a pending block without data when the lead has no dossier yet', () => {
		const { container } = renderDetail({ detail: { ...detail, company: null } })
		const card = container.querySelector('[data-section="company"]')
		expect(
			card
				?.querySelector('[data-company-state]')
				?.getAttribute('data-company-state'),
		).toBe('pending')
		expect(card?.textContent).toContain('Jamais mise à jour')
		expect(screen.getByRole('button', { name: 'Enrichir' })).toBeTruthy()
	})

	it('asks for a refresh with the Enrichir button and disables it while pending', () => {
		const onRefreshCompany = vi.fn()
		const first = renderDetail({
			detail: { ...detail, company: completeCompany },
			onRefreshCompany,
		})
		fireEvent.click(screen.getByRole('button', { name: 'Enrichir' }))
		expect(onRefreshCompany).toHaveBeenCalledTimes(1)
		first.unmount()

		renderDetail({
			detail: { ...detail, company: completeCompany },
			refreshingCompany: true,
		})
		const busy = screen.getByRole('button', { name: 'Enrichissement…' })
		expect((busy as HTMLButtonElement).disabled).toBe(true)
		cleanup()

		renderDetail({
			detail: { ...detail, company: { ...completeCompany, state: 'pending' } },
		})
		expect(
			(
				screen.getByRole('button', {
					name: 'Enrichissement…',
				}) as HTMLButtonElement
			).disabled,
		).toBe(true)
	})

	it('shows the refresh error', () => {
		renderDetail({ refreshCompanyError: 'refus du serveur' })
		expect(screen.getByText('refus du serveur')).toBeTruthy()
	})

	it('names the basis of the probable country', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				facts: { ...detail.facts, country_basis: 'head_office' },
			},
		})
		expect(
			container.querySelector('[data-section="facts"]')?.textContent,
		).toContain('Pays probableFrance (siège)')
	})

	it('asks for a status change and renders the rejection dialog slot', () => {
		const onChangeStatus = vi.fn()
		renderDetail({ onChangeStatus })
		expect(screen.getByText('rejection-dialog')).toBeTruthy()
		fireEvent.click(screen.getByRole('button', { name: 'Rejeté' }))
		expect(onChangeStatus).toHaveBeenCalledWith('rejected')
	})

	it('shows the error state', () => {
		renderDetail({ state: 'error', detail: null, errorMessage: 'not found' })
		expect(screen.getByRole('alert').textContent).toContain('not found')
	})

	it('shows no exclusion block for a lead nobody excluded', () => {
		const { container } = renderDetail()
		expect(container.querySelector('[data-section="exclusion"]')).toBeNull()
	})

	it('shows a prominent exclusion block with the option, the probability and the quote', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: {
					...detail.lead,
					verdict: 'ecarte',
					verdict_reason: 'excluded',
					exclusions: [
						{
							agent_id: 'exclusions',
							agent_name: 'Exclusions',
							question_id: 'concurrent',
							option: 'oui',
							label: 'Concurrent',
							probability: 85,
							evidence: 'Nous sommes un éditeur de solutions IAM.',
							evidence_source: 'https://acme.test/about',
						},
					],
				},
			},
		})
		const block = container.querySelector('[data-section="exclusion"]')
		expect(block?.textContent).toContain(
			"Exclu par l'agent « Exclusions » : Concurrent (85 %)",
		)
		expect(
			block?.querySelector('[data-part="exclusion-quote"]')?.textContent,
		).toBe('« Nous sommes un éditeur de solutions IAM. »')
		expect(block?.querySelector('a')?.getAttribute('href')).toBe(
			'https://acme.test/about',
		)
		expect(
			container.querySelector('[data-verdict-reason="excluded"]')?.textContent,
		).toBe('Écarté : un agent de qualification a exclu le lead')
	})

	it('lists every exclusion of the lead', () => {
		const exclusion = {
			agent_id: 'exclusions',
			agent_name: 'Exclusions',
			question_id: 'concurrent',
			option: 'oui',
			label: 'Concurrent',
			probability: 85,
			evidence: 'Nous sommes un concurrent.',
			evidence_source: 'lead',
		}
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: {
					...detail.lead,
					exclusions: [
						exclusion,
						{
							...exclusion,
							question_id: 'client_actuel',
							label: 'Client actuel',
						},
					],
				},
			},
		})
		expect(
			container.querySelectorAll('[data-section="exclusion"] [data-exclusion]'),
		).toHaveLength(2)
		expect(
			screen.getAllByText('Faits calculés par le système').length,
		).toBeGreaterThan(0)
	})

	it('shows the agreement of the samples under each measure', () => {
		const [first, second] = detail.qualification_results
		const answer = first.answers[0]
		const { container } = renderDetail({
			detail: {
				...detail,
				qualification_results: [
					{
						...first,
						provenance: { ...first.provenance, samples: 3 },
						answers: [{ ...answer, agreement: 2 / 3 }],
					},
					second,
				],
			},
		})
		const measure = container.querySelector('[data-question="pertinence"]')
		expect(measure?.querySelector('[data-part="agreement"]')?.textContent).toBe(
			'Accord : 2/3',
		)
	})

	it('shows no agreement for a single-sample answer', () => {
		const { container } = renderDetail()
		expect(container.querySelector('[data-part="agreement"]')).toBeNull()
		expect(container.querySelector('[data-part="escalated"]')).toBeNull()
	})

	it('badges an escalated result with its provider', () => {
		const [first, second] = detail.qualification_results
		const { container } = renderDetail({
			detail: {
				...detail,
				qualification_results: [
					{ ...first, escalated: true, provider_name: 'Claude' },
					second,
				],
			},
		})
		const card = container.querySelector('[data-agent="pertinence"]')
		expect(card?.querySelector('[data-part="escalated"]')?.textContent).toBe(
			'Escaladé vers Claude',
		)
		expect(
			container
				.querySelector('[data-agent="maturite"]')
				?.querySelector('[data-part="escalated"]'),
		).toBeNull()
	})
	describe('company entity', () => {
		const entity: NonNullable<LeadDetailData['company']> = {
			state: 'complete',
			siren: '552081317',
			legal_name: 'ACME SAS',
			official: null,
			pages: [],
			ignored_links: 0,
			built_at: '2026-10-06T10:00:00Z',
			next_attempt_at: '2027-01-04T10:00:00Z',
			error: null,
			siblings: [
				{
					id: 's1',
					domain: 'acme.fr',
					profile: { id: 'p1', name: 'Fintech' },
					verdict: 'fiable',
					confidence: 81,
				},
				{
					id: 's2',
					domain: 'acme.com',
					profile: { id: 'p2', name: 'Cyber' },
					verdict: null,
					confidence: null,
				},
			],
		}

		it('shows the legal name, the SIREN and the sibling domains as links', () => {
			const onOpenLead = vi.fn()
			const { container } = renderDetail({
				detail: { ...detail, company: entity },
				onOpenLead,
			})
			const line = container.querySelector('[data-section="company-entity"]')
			expect(line?.textContent).toContain('Entreprise')
			expect(line?.textContent).toContain('ACME SAS')
			expect(line?.textContent).toContain('SIREN 552 081 317')
			const links = Array.from(line?.querySelectorAll('a') ?? [])
			expect(links.map((link) => link.textContent)).toEqual([
				'acme.fr',
				'acme.com · Cyber',
			])
			expect(links.map((link) => link.getAttribute('href'))).toEqual([
				'/leads/s1',
				'/leads/s2',
			])
			fireEvent.click(links[0] as HTMLElement)
			expect(onOpenLead).toHaveBeenCalledWith('s1')
			expect(line?.textContent).toMatch(/81\s%/)
		})

		it('shows no entity line without a resolved SIREN', () => {
			const { container } = renderDetail({
				detail: {
					...detail,
					company: { ...entity, legal_name: null, siblings: [] },
				},
			})
			expect(
				container.querySelector('[data-section="company-entity"]'),
			).toBeNull()
		})

		it('shows only the legal name when the company has no sibling', () => {
			const { container } = renderDetail({
				detail: { ...detail, company: { ...entity, siblings: [] } },
			})
			const line = container.querySelector('[data-section="company-entity"]')
			expect(line?.textContent).toContain('ACME SAS')
			expect(line?.textContent).not.toContain('Autres domaines')
		})

		it('lists the aggregated domains in the facts', () => {
			const { container } = renderDetail({
				detail: {
					...detail,
					facts: { ...detail.facts, sibling_domains: ['acme.fr'] },
				},
			})
			const facts = container.querySelector('[data-section="facts"]')
			expect(facts?.textContent).toContain('Domaines regroupésacme.fr')
			expect(facts?.textContent).toContain('Signaux indépendants1')
		})
	})

	it('shows the independent signals and the decayed volume in the calculation', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: {
					...detail.lead,
					evidence: {
						strength: 35,
						components: {
							volume: 20,
							diversity: 75,
							environment: 100,
							strength: 24.2,
						},
						signal_count: 5,
						independent_signals: 3,
						decayed_signals: 1.5,
						source_count: 1,
						kind_count: 2,
						signal_score: 12.1,
						only_non_production: false,
					},
				},
			},
		})
		const calculation = container.querySelector(
			'[data-section="confidence-calculation"]',
		)
		const independent = calculation?.querySelector(
			'[data-evidence-signals="independent"]',
		)
		expect(independent?.textContent).toContain('Signaux indépendants')
		expect(independent?.textContent).toContain('3 sur 5')
		const decayed = calculation?.querySelector(
			'[data-evidence-signals="decayed"]',
		)
		expect(decayed?.textContent).toContain('Volume après décroissance')
		expect(decayed?.textContent).toContain('1,5 sur 3')
	})
})
