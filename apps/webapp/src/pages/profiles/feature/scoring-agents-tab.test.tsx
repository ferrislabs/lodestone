import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { useState } from 'react'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import type { JsonObject } from '#/lib/schema-form'
import { DefinitionTabContent } from '#/pages/profiles/feature/definition-form'
import { newAgent } from '#/pages/profiles/lib/agents'
import {
	DEFAULT_AGGREGATOR_PROMPT,
	isUnknownOption,
	newQualificationAgent,
	optionsOf,
	type ProviderOption,
	scoringList,
} from '#/pages/profiles/lib/qualification'
import {
	criteriaOf,
	newScoringAgent,
} from '#/pages/profiles/lib/scoring-agents'
import {
	type EditorTab,
	editorTabItems,
	TAB_LABELS,
} from '#/pages/profiles/lib/tabs'
import type { Problem } from '#/pages/profiles/lib/types'
import { TabBar } from '#/pages/profiles/ui/tab-bar'

class NoopMediaQuery {
	matches = false
	addEventListener() {}
	removeEventListener() {}
}

class NoopResizeObserver {
	observe() {}
	unobserve() {}
	disconnect() {}
}

beforeAll(() => {
	vi.stubGlobal('ResizeObserver', NoopResizeObserver)
	vi.stubGlobal('matchMedia', () => new NoopMediaQuery())
})

afterEach(cleanup)

const PROVIDERS: ProviderOption[] = [
	{ id: 'p-laya', name: 'Laya local', enabled: true },
	{ id: 'p-jev', name: 'Jev local', enabled: false },
]

function base(): JsonObject {
	const taken = new Set(['ats', 'pertinence'])
	return {
		schema_version: 11,
		identity: { offer: 'o', target_customer: 't' },
		agents: [newAgent('ats', [])],
		qualification_agents: [
			newQualificationAgent('pertinence', new Set(['ats'])),
		],
		scoring_agents: [newScoringAgent(taken)],
		aggregator: { provider_id: null, system_prompt: DEFAULT_AGGREGATOR_PROMPT },
		verdict: { reliable_from: 70, discard_below: 30 },
		scoring: { factors: [] },
		contact_ranking: { preferred_local_parts: [] },
		llm: { qualification_threshold: 50, relevance_criteria: 'r' },
	}
}

function Harness({
	initial = base(),
	problems = [],
	startTab = 'scoring',
	onValue,
}: {
	initial?: JsonObject
	problems?: readonly Problem[]
	startTab?: EditorTab
	onValue?: (value: JsonObject) => void
}) {
	const [value, setValue] = useState(initial)
	const [tab, setTab] = useState<EditorTab>(startTab)
	return (
		<>
			<TabBar tabs={editorTabItems(problems)} active={tab} onChange={setTab} />
			<DefinitionTabContent
				tab={tab}
				value={value}
				problems={problems}
				providers={PROVIDERS}
				onChange={(next) => {
					setValue(next)
					onValue?.(next)
				}}
			/>
		</>
	)
}

function sheet() {
	return within(screen.getByRole('dialog'))
}

function lastValue(spy: ReturnType<typeof vi.fn>): JsonObject {
	return spy.mock.calls.at(-1)?.[0] as JsonObject
}

function openFirst() {
	fireEvent.click(screen.getByRole('button', { name: 'Modifier Score global' }))
}

describe('scoring tab', () => {
	it('keeps Scoring as a tab and lists the agents above the signal weights', () => {
		render(<Harness />)
		expect(screen.getByRole('tab', { name: TAB_LABELS.scoring })).toBeTruthy()
		const agents = screen.getByRole('region', { name: 'Agents de scoring' })
		const weights = screen.getByRole('region', {
			name: 'Poids des signaux (sourcing)',
		})
		expect(
			agents.compareDocumentPosition(weights) &
				Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy()
		expect(
			within(weights).getByRole('heading', {
				name: 'Poids des signaux (sourcing)',
			}),
		).toBeTruthy()
		expect(within(weights).getByLabelText('Facteurs de scoring')).toBeTruthy()
	})

	it('lays the cards three per row with the help panel on the right', () => {
		render(<Harness />)
		const layout = screen.getByTestId('help-layout')
		expect(layout.className).toContain('lg:grid-cols-4')
		const grid = document.querySelector(
			'[data-scoring-agent="0"]',
		)?.parentElement
		expect(grid?.className).toContain('xl:grid-cols-3')
		const panel = screen.getAllByTestId('help-panel')[0]
		expect(panel?.getAttribute('aria-label')).toBe('Scoring')
		expect(panel?.parentElement?.className).toContain('lg:col-span-1')
		expect(layout.contains(panel ?? null)).toBe(true)
	})

	it('explains the flow and shows the imposed contract in the help panel', () => {
		render(<Harness />)
		const panel = within(screen.getAllByTestId('help-panel')[0] as HTMLElement)
		expect(panel.getByText(/Agents de scoring : chacun décide/)).toBeTruthy()
		expect(panel.getByText(/une exclusion décidée/)).toBeTruthy()
		const example = screen.getByTestId('scoring-contract-example').textContent
		expect(example).toContain('"score": 0')
		expect(example).toContain('"reasons"')
		expect(example).toContain('fiable | a_verifier | ecarte')
	})

	it('shows each agent as a card with its summary', () => {
		render(<Harness />)
		const card = document.querySelector('[data-scoring-agent="0"]')
		expect(card?.textContent).toContain('Score global')
		expect(card?.textContent).toContain(
			'3 critères · fournisseur : par défaut · poids 5 · 1 échantillon',
		)
		expect(
			screen.getByRole('button', { name: 'Ajouter un agent de scoring' }),
		).toBeTruthy()
	})

	it('says the verdict stays pending without any scoring agent', () => {
		render(<Harness initial={{ ...base(), scoring_agents: [] }} />)
		expect(screen.getByText(/Aucun agent de scoring/)).toBeTruthy()
		expect(
			screen.getByText(/les leads restent en attente du scoring/),
		).toBeTruthy()
	})

	it('adds an agent with unique ids and opens its sheet', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(
			screen.getByRole('button', { name: 'Ajouter un agent de scoring' }),
		)
		const agents = scoringList(lastValue(seen))
		expect(agents).toHaveLength(2)
		expect(agents[1]).toMatchObject({
			id: 'score-global-2',
			name: 'Score global',
			provider_id: null,
			weight: 5,
			samples: 1,
		})
		expect(sheet().getByLabelText('Nom')).toHaveProperty(
			'value',
			'Score global',
		)
	})

	it('edits the agent section of the sheet', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		openFirst()
		fireEvent.change(sheet().getByLabelText('Nom'), {
			target: { value: 'Décideur prudent' },
		})
		let agent = scoringList(lastValue(seen))[0]
		expect(agent).toMatchObject({
			name: 'Décideur prudent',
			id: 'decideur-prudent',
		})
		fireEvent.change(sheet().getByLabelText('Ce que cet agent évalue'), {
			target: { value: 'Décide avec prudence.' },
		})
		fireEvent.change(sheet().getByLabelText('Fournisseur LLM'), {
			target: { value: 'p-laya' },
		})
		fireEvent.change(sheet().getByLabelText('Échantillons'), {
			target: { value: '3' },
		})
		fireEvent.change(sheet().getByLabelText("Température d'échantillonnage"), {
			target: { value: '0.4' },
		})
		fireEvent.change(
			sheet().getByLabelText('Prompt système', { selector: 'textarea' }),
			{ target: { value: 'Sois prudent.' } },
		)
		fireEvent.click(
			sheet().getAllByRole('radio', { name: '8' })[0] as HTMLElement,
		)
		agent = scoringList(lastValue(seen))[0]
		expect(agent).toMatchObject({
			description: 'Décide avec prudence.',
			provider_id: 'p-laya',
			samples: 3,
			sampling_temperature: 0.4,
			system_prompt: 'Sois prudent.',
			weight: 8,
		})
	})

	it('offers the Synthèse default as the empty provider', () => {
		render(<Harness />)
		openFirst()
		const select = sheet().getByLabelText(
			'Fournisseur LLM',
		) as HTMLSelectElement
		expect(select.value).toBe('')
		expect(
			within(select).getByRole('option', {
				name: 'Synthèse (fournisseur par défaut)',
			}),
		).toBeTruthy()
	})

	it('labels the sections Agent, Prompt système, Critères and the contract preview', () => {
		render(<Harness />)
		openFirst()
		for (const name of [
			'Agent',
			'Prompt système',
			'Critères',
			'Aperçu du JSON contractuel',
		]) {
			expect(sheet().getByRole('region', { name })).toBeTruthy()
		}
		expect(sheet().queryByText("Axes d'analyse")).toBeNull()
	})

	it('edits criteria like axes, without any exclusion toggle', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		openFirst()
		const first = sheet().getByRole('region', { name: 'Critère 1' })
		expect(first).toBeTruthy()
		expect(sheet().queryByText('Exclut le lead')).toBeNull()
		expect(sheet().queryByText('Exclusion')).toBeNull()
		fireEvent.change(within(first).getByLabelText('Intitulé du critère'), {
			target: { value: 'Cible atteinte ?' },
		})
		const criteria = criteriaOf(scoringList(lastValue(seen))[0])
		expect(criteria[0]).toMatchObject({
			wording: 'Cible atteinte ?',
			id: 'adequation_offre',
		})
		fireEvent.click(sheet().getByRole('button', { name: 'Ajouter un critère' }))
		expect(criteriaOf(scoringList(lastValue(seen))[0])).toHaveLength(4)
		expect(sheet().getByRole('region', { name: 'Critère 4' })).toBeTruthy()
		fireEvent.click(
			within(sheet().getByRole('region', { name: 'Critère 4' })).getByRole(
				'button',
				{ name: 'Supprimer le critère 4' },
			),
		)
		expect(criteriaOf(scoringList(lastValue(seen))[0])).toHaveLength(3)
	})

	it('adds and removes options and keeps the unknown option pinned', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		openFirst()
		fireEvent.click(
			sheet().getByRole('button', { name: 'Ajouter une option au critère 1' }),
		)
		const options = optionsOf(criteriaOf(scoringList(lastValue(seen))[0])[0])
		expect(options).toHaveLength(5)
		expect(isUnknownOption(options.at(-1))).toBe(true)
		const first = sheet().getByRole('region', { name: 'Critère 1' })
		fireEvent.change(within(first).getByLabelText("Score de l'option 1"), {
			target: { value: '0.9' },
		})
		expect(
			optionsOf(criteriaOf(scoringList(lastValue(seen))[0])[0])[0],
		).toMatchObject({ score: 0.9 })
	})

	it('previews the imposed contract read-only next to the criteria schema', () => {
		render(<Harness />)
		openFirst()
		fireEvent.click(
			sheet().getByRole('button', { name: 'Aperçu du JSON contractuel' }),
		)
		const imposed = JSON.parse(
			screen.getByTestId('contract-imposed').textContent ?? '{}',
		)
		expect(Object.keys(imposed)).toEqual(['score', 'verdict', 'reasons'])
		expect(imposed.score).toMatchObject({
			type: 'integer',
			minimum: 0,
			maximum: 100,
		})
		expect(imposed.verdict.required).toEqual([
			'choice',
			'confidence',
			'probabilities',
		])
		expect(imposed.verdict.properties.probabilities.required).toEqual([
			'fiable',
			'a_verifier',
			'ecarte',
		])
		expect(imposed.reasons).toMatchObject({ minItems: 1, maxItems: 5 })
		const criteria = JSON.parse(
			screen.getByTestId('contract-criteria').textContent ?? '{}',
		)
		expect(criteria.required).toEqual([
			'adequation_offre',
			'solidite_preuves',
			'priorite_commerciale',
		])
		expect(screen.getByText(/Partie imposée par l'application/)).toBeTruthy()
		expect(sheet().queryByRole('textbox', { name: /imposé/i })).toBeNull()
	})

	it('switches an agent off and removes it after a confirmation', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(
			screen.getByRole('switch', {
				name: 'Agent de scoring Score global activé',
			}),
		)
		expect(scoringList(lastValue(seen))[0]).toMatchObject({ enabled: false })
		fireEvent.click(
			screen.getByRole('button', {
				name: "Supprimer l'agent de scoring Score global",
			}),
		)
		expect(scoringList(lastValue(seen))).toHaveLength(1)
		fireEvent.click(
			screen.getByRole('button', { name: 'Confirmer la suppression' }),
		)
		expect(scoringList(lastValue(seen))).toHaveLength(0)
	})

	it('flags the problems of an agent on its card, in its sheet and on the tab', () => {
		const problems: Problem[] = [
			{ path: 'scoring_agents[0].weight', message: 'Doit être de 1 à 10' },
			{ path: 'scoring_agents[0].criteria[1].id', message: 'Doit être unique' },
		]
		render(<Harness problems={problems} />)
		const card = document.querySelector('[data-scoring-agent="0"]')
		expect(card?.textContent).toContain('2 problèmes')
		expect(screen.getByRole('tab', { name: /Scoring/ }).textContent).toContain(
			'2',
		)
		openFirst()
		expect(sheet().getByText('Doit être de 1 à 10')).toBeTruthy()
		expect(sheet().getByText('Doit être unique')).toBeTruthy()
	})
})
