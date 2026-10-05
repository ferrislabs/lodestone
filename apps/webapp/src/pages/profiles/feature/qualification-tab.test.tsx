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
	newQualificationAgent,
	optionsOf,
	type ProviderOption,
	qualificationList,
	questionsOf,
} from '#/pages/profiles/lib/qualification'
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
	return {
		schema_version: 4,
		identity: { offer: 'o', target_customer: 't' },
		agents: [newAgent('ats', [])],
		qualification_agents: [
			newQualificationAgent('pertinence', new Set(['ats'])),
		],
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
	startTab = 'qualification',
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

function closeSheet() {
	fireEvent.keyDown(document, { key: 'Escape' })
}

function lastValue(spy: ReturnType<typeof vi.fn>): JsonObject {
	return spy.mock.calls.at(-1)?.[0] as JsonObject
}

describe('qualification tab', () => {
	it('renames the first tab Sourcing and adds Qualification before Scoring', () => {
		render(<Harness />)
		const labels = screen
			.getAllByRole('tab')
			.map((tab) => tab.textContent?.trim())
		expect(labels).toEqual([
			TAB_LABELS.identity,
			'Sourcing',
			'Qualification',
			'Scoring',
			'Réglages',
		])
	})

	it('renders the help panel with the flow and the contract JSON', () => {
		render(<Harness />)
		const panel = screen.getByTestId('help-panel')
		expect(
			within(panel).getByText('Confiance calculée', { exact: false }),
		).toBeTruthy()
		expect(within(panel).getByText(/Agrégateur : rédige/)).toBeTruthy()
		expect(screen.getByTestId('contract-example').textContent).toContain(
			'"probabilities"',
		)
		expect(screen.getByTestId('help-layout').className).toContain(
			'lg:grid-cols-4',
		)
	})

	it('lists the agents as cards with a summary', () => {
		render(<Harness />)
		const card = document.querySelector('[data-qualification-agent="0"]')
		expect(card?.textContent).toContain('Pertinence')
		expect(card?.textContent).toContain(
			'1 question · fournisseur : par défaut · poids 5',
		)
		expect(
			screen.getByRole('button', { name: 'Ajouter un agent de qualification' }),
		).toBeTruthy()
	})

	it('shows an empty state without agents', () => {
		render(<Harness initial={{ ...base(), qualification_agents: [] }} />)
		expect(screen.getByText(/Aucun agent de qualification/)).toBeTruthy()
	})

	it('adds an agent from a template and opens it', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(
			screen.getByRole('button', { name: 'Ajouter un agent de qualification' }),
		)
		const picker = screen.getByLabelText(
			"Choisir un modèle d'agent de qualification",
		)
		for (const label of [
			'Pertinence',
			'Maturité technique',
			"Type d'organisation",
			'Vide',
		]) {
			expect(within(picker).getByText(label)).toBeTruthy()
		}
		fireEvent.click(within(picker).getByText('Maturité technique'))
		const agents = qualificationList(lastValue(seen))
		expect(agents).toHaveLength(2)
		expect(agents[1]).toMatchObject({
			id: 'maturite-technique',
			name: 'Maturité technique',
			provider_id: null,
		})
		expect(sheet().getByLabelText('Nom')).toHaveProperty(
			'value',
			'Maturité technique',
		)
		expect(
			sheet().getByLabelText('Prompt système', { selector: 'textarea' }),
		).toBeTruthy()
	})

	it('adds a blank agent with one empty question', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(
			screen.getByRole('button', { name: 'Ajouter un agent de qualification' }),
		)
		fireEvent.click(screen.getByText('Vide'))
		const agent = qualificationList(lastValue(seen))[1]
		expect(questionsOf(agent)).toHaveLength(1)
		expect(optionsOf(questionsOf(agent)[0])).toHaveLength(2)
	})

	it('edits name, enabled, weight and prompt', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		fireEvent.change(sheet().getByLabelText('Nom'), {
			target: { value: 'Pertinence IAM' },
		})
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			name: 'Pertinence IAM',
			id: 'pertinence-iam',
		})
		fireEvent.click(sheet().getByLabelText('Activé'))
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			enabled: false,
		})
		const group = sheet().getByRole('radiogroup', { name: 'Poids' })
		fireEvent.click(within(group).getByRole('radio', { name: '8' }))
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({ weight: 8 })
		fireEvent.change(
			sheet().getByLabelText('Prompt système', { selector: 'textarea' }),
			{ target: { value: 'Nouveau prompt' } },
		)
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			system_prompt: 'Nouveau prompt',
		})
	})

	it('lists declared providers and the default in the agent select', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const select = sheet().getByLabelText(
			'Fournisseur LLM',
		) as HTMLSelectElement
		expect(
			Array.from(select.options).map((option) => option.textContent),
		).toEqual(['Fournisseur par défaut', 'Laya local', 'Jev local (désactivé)'])
		expect(select.value).toBe('')
		fireEvent.change(select, { target: { value: 'p-laya' } })
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			provider_id: 'p-laya',
		})
		fireEvent.change(select, { target: { value: '' } })
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			provider_id: null,
		})
	})

	it('keeps an unknown provider selectable and flagged', () => {
		const initial = base()
		initial.qualification_agents = [
			{
				...newQualificationAgent('pertinence', new Set()),
				provider_id: 'gone',
			},
		]
		render(<Harness initial={initial} />)
		expect(
			screen.getByText('1 question · fournisseur : introuvable · poids 5'),
		).toBeTruthy()
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const select = sheet().getByLabelText(
			'Fournisseur LLM',
		) as HTMLSelectElement
		expect(select.value).toBe('gone')
		expect(
			Array.from(select.options).map((option) => option.textContent),
		).toContain('Fournisseur introuvable')
	})

	it('edits questions and options', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		fireEvent.click(
			sheet().getByRole('button', { name: 'Ajouter une question' }),
		)
		let question = within(sheet().getByLabelText('Question 2'))
		fireEvent.change(question.getByLabelText('Libellé'), {
			target: { value: 'Quel budget ?' },
		})
		expect(questionsOf(qualificationList(lastValue(seen))[0])[1]).toMatchObject(
			{
				wording: 'Quel budget ?',
				id: 'quel-budget',
			},
		)
		fireEvent.click(
			question.getByRole('button', {
				name: 'Ajouter une option à la question 2',
			}),
		)
		question = within(sheet().getByLabelText('Question 2'))
		expect(
			optionsOf(questionsOf(qualificationList(lastValue(seen))[0])[1]),
		).toHaveLength(3)
		const scores = question.getAllByLabelText('Score')
		fireEvent.change(scores[0] as HTMLElement, { target: { value: '0.7' } })
		fireEvent.change(question.getAllByLabelText('Valeur')[0] as HTMLElement, {
			target: { value: 'eleve' },
		})
		fireEvent.change(
			question.getAllByLabelText("Libellé de l'option")[0] as HTMLElement,
			{ target: { value: 'Élevé' } },
		)
		const options = optionsOf(
			questionsOf(qualificationList(lastValue(seen))[0])[1],
		)
		expect(options[0]).toEqual({ value: 'eleve', label: 'Élevé', score: 0.7 })
		fireEvent.click(
			question.getByRole('button', {
				name: "Supprimer l'option 3 de la question 2",
			}),
		)
		expect(
			optionsOf(questionsOf(qualificationList(lastValue(seen))[0])[1]),
		).toHaveLength(2)
		fireEvent.click(
			sheet().getByRole('button', { name: 'Supprimer la question 2' }),
		)
		expect(questionsOf(qualificationList(lastValue(seen))[0])).toHaveLength(1)
	})

	it('score inputs step by a tenth between 0 and 1', () => {
		render(<Harness />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const score = sheet().getAllByLabelText('Score')[0] as HTMLInputElement
		expect(score.step).toBe('0.1')
		expect(score.min).toBe('0')
		expect(score.max).toBe('1')
	})

	it('forbids fewer than two options and a lone question removal', () => {
		const initial = base()
		const agent = newQualificationAgent('pertinence', new Set())
		const question = questionsOf(agent)[0] as JsonObject
		initial.qualification_agents = [
			{
				...agent,
				questions: [{ ...question, options: optionsOf(question).slice(0, 2) }],
			},
		]
		render(<Harness initial={initial} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		expect(
			(
				sheet().getByRole('button', {
					name: "Supprimer l'option 1 de la question 1",
				}) as HTMLButtonElement
			).disabled,
		).toBe(true)
		expect(
			(
				sheet().getByRole('button', {
					name: 'Supprimer la question 1',
				}) as HTMLButtonElement
			).disabled,
		).toBe(true)
	})

	it('previews the contract JSON schema read-only and follows the options', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		expect(screen.queryByTestId('contract-preview')).toBeNull()
		fireEvent.click(
			sheet().getByRole('button', { name: 'Aperçu du JSON contractuel' }),
		)
		const preview = screen.getByTestId('contract-preview')
		expect(preview.tagName).toBe('PRE')
		const parsed = JSON.parse(preview.textContent ?? '')
		expect(parsed.properties.answers.required).toEqual(['pertinence'])
		expect(
			parsed.properties.answers.properties.pertinence.properties.choice.enum,
		).toEqual(['forte', 'moyenne', 'faible'])
		fireEvent.click(
			sheet().getByRole('button', {
				name: 'Ajouter une option à la question 1',
			}),
		)
		expect(
			JSON.parse(screen.getByTestId('contract-preview').textContent ?? '')
				.properties.answers.properties.pertinence.properties.probabilities
				.required,
		).toEqual(['forte', 'moyenne', 'faible', 'option-4'])
		fireEvent.click(
			sheet().getByRole('button', { name: 'Aperçu du JSON contractuel' }),
		)
		expect(screen.queryByTestId('contract-preview')).toBeNull()
	})

	it('removes an agent after confirmation', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(
			screen.getByRole('button', {
				name: "Supprimer l'agent de qualification Pertinence",
			}),
		)
		fireEvent.click(
			screen.getByRole('button', { name: 'Confirmer la suppression' }),
		)
		expect(qualificationList(lastValue(seen))).toHaveLength(0)
	})

	it('toggles an agent from its card', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(
			screen.getByRole('switch', {
				name: 'Agent de qualification Pertinence activé',
			}),
		)
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			enabled: false,
		})
		closeSheet()
	})
})

describe('aggregator and thresholds', () => {
	it('edits the aggregator provider and prompt', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		const panel = within(
			screen.getByLabelText('Agrégateur', { selector: 'section' }),
		)
		const select = panel.getByLabelText('Fournisseur LLM') as HTMLSelectElement
		expect(
			Array.from(select.options).map((option) => option.textContent),
		).toEqual(['Fournisseur par défaut', 'Laya local', 'Jev local (désactivé)'])
		fireEvent.change(select, { target: { value: 'p-laya' } })
		expect(lastValue(seen).aggregator).toMatchObject({ provider_id: 'p-laya' })
		fireEvent.change(panel.getByLabelText('Prompt système'), {
			target: { value: 'Résume.' },
		})
		expect(lastValue(seen).aggregator).toMatchObject({
			system_prompt: 'Résume.',
		})
	})

	it('shows the thresholds with a three band bar', () => {
		render(<Harness />)
		expect(screen.getByLabelText('Fiable à partir de')).toHaveProperty(
			'value',
			'70',
		)
		expect(screen.getByLabelText('Écarté sous')).toHaveProperty('value', '30')
		const bar = screen.getByTestId('verdict-bar')
		const widths = Array.from(bar.children).map(
			(band) => (band as HTMLElement).style.width,
		)
		expect(widths).toEqual(['30%', '40%', '30%'])
	})

	it('edits a threshold and updates the bar', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.change(screen.getByLabelText('Fiable à partir de'), {
			target: { value: '80' },
		})
		expect(lastValue(seen).verdict).toEqual({
			reliable_from: 80,
			discard_below: 30,
		})
		const widths = Array.from(screen.getByTestId('verdict-bar').children).map(
			(band) => (band as HTMLElement).style.width,
		)
		expect(widths).toEqual(['30%', '50%', '20%'])
	})

	it('rejects a reliable threshold not above the discard threshold', () => {
		render(<Harness />)
		fireEvent.change(screen.getByLabelText('Fiable à partir de'), {
			target: { value: '20' },
		})
		expect(screen.getByRole('alert').textContent).toContain(
			'doit être supérieur',
		)
		expect(screen.queryByTestId('verdict-bar')).toBeNull()
		expect(
			screen.getByLabelText('Fiable à partir de').getAttribute('aria-invalid'),
		).toBe('true')
	})

	it('rejects an empty or out of range threshold', () => {
		render(<Harness />)
		fireEvent.change(screen.getByLabelText('Écarté sous'), {
			target: { value: '' },
		})
		expect(screen.getByRole('alert').textContent).toContain('0 à 100')
		fireEvent.change(screen.getByLabelText('Écarté sous'), {
			target: { value: '150' },
		})
		expect(screen.getByRole('alert').textContent).toContain('0 à 100')
	})
})

describe('server problems', () => {
	const problems: Problem[] = [
		{
			path: 'qualification_agents[0].questions[0].options[1].score',
			message: 'Doit être un nombre de 0 à 1',
		},
		{ path: 'aggregator.system_prompt', message: 'Ne doit pas être vide' },
		{
			path: 'verdict.reliable_from',
			message: 'Doit être supérieur au seuil « Écarté sous »',
		},
	]

	it('badges the qualification tab and the card', () => {
		render(<Harness problems={problems} />)
		const tab = screen.getByRole('tab', { name: /Qualification/ })
		expect(within(tab).getByLabelText('3 problèmes')).toBeTruthy()
		const card = document.querySelector('[data-qualification-agent="0"]')
		expect(card?.textContent).toContain('1 problème')
		expect(screen.getByText('Ne doit pas être vide')).toBeTruthy()
		expect(
			screen.getByText('Doit être supérieur au seuil « Écarté sous »'),
		).toBeTruthy()
	})

	it('shows an option score problem under its field', () => {
		render(<Harness problems={problems} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const scores = sheet().getAllByLabelText('Score')
		expect(scores[1]?.getAttribute('aria-invalid')).toBe('true')
		expect(scores[0]?.getAttribute('aria-invalid')).toBe('false')
		expect(sheet().getByText('Doit être un nombre de 0 à 1')).toBeTruthy()
	})
})
