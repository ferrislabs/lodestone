import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { LeadDetailData } from '#/pages/leads/lib/types'
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

const detail: LeadDetailData = {
	lead: {
		id: 'l1',
		profile: { id: 'p1', name: 'Fintech' },
		domain: 'acme.test',
		status: 'qualified',
		score: 42.5,
		first_seen_at: '2026-01-01T00:00:00Z',
		last_seen_at: '2026-02-01T00:00:00Z',
		updated_at: '2026-02-01T00:00:00Z',
		confidence: 72.5,
		verdict: 'a_verifier',
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
			signal_score: 12.1,
			only_non_production: true,
		},
		verdict_reason: 'non_production',
		qualified_at: '2026-02-01T00:00:00Z',
	},
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
		source_count: 1,
		sources: ['crawler'],
		first_seen_at: '2026-01-01T00:00:00Z',
		last_seen_at: '2026-02-01T00:00:00Z',
		probable_country: { code: 'FR', name: 'France' },
	},
	score_breakdown: [
		{
			signal: 'hiring_page',
			value: 'careers',
			weight: 3,
			measure: 1,
			contribution: 3,
		},
	],
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
			onChangeStatus={() => {}}
			rejectionDialog={<div>rejection-dialog</div>}
			onOpenDraft={() => {}}
			onRetrySummary={() => {}}
			retryingSummary={false}
			retrySummaryError={null}
			onRefreshCompany={() => {}}
			refreshingCompany={false}
			refreshCompanyError={null}
			changing={false}
			changeError={null}
			{...overrides}
		/>,
	)
}

describe('LeadDetail', () => {
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

	it('renders header, breakdown, signals, contacts and assessments', () => {
		renderDetail()
		expect(screen.getByRole('heading', { name: 'acme.test' })).toBeTruthy()
		expect(screen.getByText('Fintech')).toBeTruthy()
		expect(screen.getByText('Détail du score')).toBeTruthy()
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

	it('shows the verdict and the confidence of the lead', () => {
		const { container } = renderDetail()
		const badge = container.querySelector('[data-verdict]')
		expect(badge?.getAttribute('data-verdict')).toBe('a_verifier')
		expect(badge?.textContent).toBe('À vérifier')
		expect(screen.getByText('Confiance')).toBeTruthy()
		expect(screen.getByText(/^72,5\s%$/)).toBeTruthy()
	})

	it('names the signal score and splits the confidence into opinion and evidence', () => {
		renderDetail()
		expect(screen.getByText('Score des signaux')).toBeTruthy()
		expect(screen.queryByText('Score')).toBeNull()
		expect(
			screen.getByText('Avis des agents').nextSibling?.textContent,
		).toMatch(/^92,9\s%$/)
		expect(
			screen.getByText('Solidité des preuves').nextSibling?.textContent,
		).toMatch(/^35\s%$/)
	})

	it('lists the evidence components and the verdict reason in the calculation', () => {
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
			calculation?.querySelector('[data-verdict-reason]')?.textContent,
		).toBe('Plafonné à « À vérifier » : uniquement des hôtes hors production')
	})

	it('shows no verdict reason when no rule applied, and no calculation before scoring', () => {
		const { container } = renderDetail({
			detail: {
				...detail,
				lead: { ...detail.lead, verdict_reason: null },
			},
		})
		expect(container.querySelector('[data-verdict-reason]')).toBeNull()
		expect(
			container.querySelector('[data-section="confidence-calculation"]'),
		).not.toBeNull()
		cleanup()
		const bare = renderDetail({
			detail: {
				...detail,
				lead: {
					...detail.lead,
					evidence: null,
					opinion: null,
					verdict_reason: null,
				},
			},
		})
		expect(
			bare.container.querySelector('[data-section="confidence-calculation"]'),
		).toBeNull()
		expect(screen.getByText('Avis des agents').nextSibling?.textContent).toBe(
			'—',
		)
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
		built_at: '2026-10-06T10:00:00Z',
		next_attempt_at: '2027-01-04T10:00:00Z',
		error: null,
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
})
