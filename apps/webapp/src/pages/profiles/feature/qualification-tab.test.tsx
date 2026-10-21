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
import type { CalibrationStates } from '#/pages/profiles/lib/agent-calibration'
import { newAgent } from '#/pages/profiles/lib/agents'
import {
	DEFAULT_AGGREGATOR_PROMPT,
	isUnknownOption,
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
		schema_version: 14,
		identity: { offer: 'o', target_customer: 't' },
		agents: [newAgent('ats', [])],
		qualification_agents: [
			newQualificationAgent('pertinence', new Set(['ats'])),
		],
		aggregator: { provider_id: null, system_prompt: DEFAULT_AGGREGATOR_PROMPT },
		verdict: { reliable_from: 70, discard_below: 30 },
		contact_ranking: { preferred_local_parts: [] },
		language: 'fr',
	}
}

function Harness({
	initial = base(),
	problems = [],
	startTab = 'qualification',
	calibration,
	onValue,
}: {
	initial?: JsonObject
	problems?: readonly Problem[]
	startTab?: EditorTab
	calibration?: CalibrationStates
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
				calibration={calibration}
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
			"1 axe d'analyse · fournisseur : par défaut · poids 5",
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
			'Environnement',
			"Type et taille d'organisation",
			'Marché',
			'Opportunité',
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

	it('inserts a multi-axis template and fills the sheet', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(
			screen.getByRole('button', { name: 'Ajouter un agent de qualification' }),
		)
		fireEvent.click(
			within(
				screen.getByLabelText("Choisir un modèle d'agent de qualification"),
			).getByText('Marché'),
		)
		const agent = qualificationList(lastValue(seen))[1]
		expect(agent).toMatchObject({ id: 'marche', name: 'Marché', weight: 5 })
		expect(questionsOf(agent)).toHaveLength(2)
		expect(sheet().getByLabelText('Nom')).toHaveProperty('value', 'Marché')
		expect(sheet().getByLabelText('Axe 1')).toBeTruthy()
		expect(sheet().getByLabelText('Axe 2')).toBeTruthy()
		expect(
			(
				sheet().getByLabelText('Prompt système', {
					selector: 'textarea',
				}) as HTMLTextAreaElement
			).value,
		).toContain('Zone visée : France et Europe francophone.')
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
		expect(optionsOf(questionsOf(agent)[0])).toHaveLength(3)
	})

	it('edits name, enabled, weight and prompt', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		fireEvent.change(sheet().getByLabelText('Nom'), {
			target: { value: 'Pertinence IAM' },
		})
		fireEvent.change(sheet().getByLabelText('Ce que cet agent évalue'), {
			target: { value: 'Le fit avec Keycloak' },
		})
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			description: 'Le fit avec Keycloak',
		})
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			name: 'Pertinence IAM',
			id: 'pertinence-iam',
		})
		fireEvent.click(sheet().getByLabelText('Activé'))
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			enabled: false,
		})
		const group = sheet().getByRole('radiogroup', { name: "Poids de l'agent" })
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
			screen.getByText("1 axe d'analyse · fournisseur : introuvable · poids 5"),
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

	it('groups the editor in named sections without any Avancé block', () => {
		render(<Harness />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const form = sheet()
		const headings = form
			.getAllByRole('heading', { level: 3 })
			.map((heading) => heading.textContent)
		expect(headings).toEqual(['Agent', 'Prompt système', "Axes d'analyse"])
		expect(
			form.getByRole('button', { name: 'Aperçu du JSON contractuel' }),
		).toBeTruthy()
		expect(form.queryByText('Avancé')).toBeNull()
		expect(form.queryByText('Questions')).toBeNull()
		expect(form.queryByText(/Question \d/)).toBeNull()
		expect(form.getByLabelText('Axe 1')).toBeTruthy()
		expect(
			form.getByRole('button', { name: "Ajouter un axe d'analyse" }),
		).toBeTruthy()
		expect(form.getByText("Poids de l'axe")).toBeTruthy()
	})

	it('shows the agent identifier under its name and edits it', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const id = sheet().getByLabelText('Identifiant') as HTMLInputElement
		expect(id.value).toBe('pertinence')
		expect(id.className).toContain('font-mono')
		fireEvent.change(id, { target: { value: 'pertinence-iam' } })
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			id: 'pertinence-iam',
		})
	})

	it('shows the description field with its counter', () => {
		render(<Harness />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const field = sheet().getByLabelText(
			'Ce que cet agent évalue',
		) as HTMLTextAreaElement
		expect(field.tagName).toBe('TEXTAREA')
		expect(field.rows).toBe(2)
		expect(field.value).toBe("La pertinence du lead pour l'offre du profil.")
		expect(sheet().getByText(/45 \/ 300 caractères/)).toBeTruthy()
	})

	it('toggles an option as excluding, never the unknown one', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const axis = within(sheet().getByLabelText('Axe 1'))
		const toggles = axis.getAllByRole('switch', { name: /^Exclut le lead/ })
		expect(toggles).toHaveLength(4)
		expect(
			toggles.map((toggle) => (toggle as HTMLButtonElement).disabled),
		).toEqual([false, false, false, true])
		fireEvent.click(toggles[2] as HTMLElement)
		const options = optionsOf(
			questionsOf(qualificationList(lastValue(seen))[0])[0],
		)
		expect(options.map((item) => (item as JsonObject).excludes)).toEqual([
			false,
			false,
			true,
			false,
		])
		expect(
			axis
				.getAllByRole('switch', { name: /^Exclut le lead/ })[2]
				?.getAttribute('aria-checked'),
		).toBe('true')
	})

	it('edits the exclusion threshold as a percentage', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const field = sheet().getByLabelText(
			"Seuil d'exclusion",
		) as HTMLInputElement
		expect(field.value).toBe('60')
		expect(field.min).toBe('50')
		expect(field.max).toBe('100')
		fireEvent.change(field, { target: { value: '80' } })
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			exclusion_threshold: 0.8,
		})
		fireEvent.change(field, { target: { value: '' } })
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			exclusion_threshold: null,
		})
	})

	it('shows the problem on a threshold outside 50 to 100', () => {
		render(
			<Harness
				problems={[
					{
						path: 'qualification_agents[0].exclusion_threshold',
						message: 'Doit être un pourcentage entier de 50 à 100',
					},
				]}
			/>,
		)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		expect(
			sheet().getByText('Doit être un pourcentage entier de 50 à 100'),
		).toBeTruthy()
		expect(
			sheet().getByLabelText("Seuil d'exclusion").getAttribute('aria-invalid'),
		).toBe('true')
	})

	it('edits the samples and the sampling temperature of an agent', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const samples = sheet().getByLabelText('Échantillons') as HTMLInputElement
		expect(samples.value).toBe('3')
		expect(samples.min).toBe('1')
		expect(samples.max).toBe('5')
		fireEvent.change(samples, { target: { value: '5' } })
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({ samples: 5 })
		fireEvent.change(samples, { target: { value: '' } })
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			samples: null,
		})
		const temperature = sheet().getByLabelText(
			"Température d'échantillonnage",
		) as HTMLInputElement
		expect(temperature.value).toBe('0.7')
		expect(temperature.max).toBe('1.5')
		fireEvent.change(temperature, { target: { value: '1.2' } })
		expect(qualificationList(lastValue(seen))[0]).toMatchObject({
			sampling_temperature: 1.2,
		})
	})

	it('shows the problems of the sampling fields', () => {
		render(
			<Harness
				problems={[
					{
						path: 'qualification_agents[0].samples',
						message: 'Doit être un entier de 1 à 5',
					},
					{
						path: 'qualification_agents[0].sampling_temperature',
						message: 'Doit être un nombre de 0 à 1.5',
					},
				]}
			/>,
		)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		expect(sheet().getByText('Doit être un entier de 1 à 5')).toBeTruthy()
		expect(sheet().getByText('Doit être un nombre de 0 à 1.5')).toBeTruthy()
		expect(
			sheet().getByLabelText('Échantillons').getAttribute('aria-invalid'),
		).toBe('true')
	})

	it('shows the escalation panel disabled by default with the default band', () => {
		render(<Harness />)
		const panel = within(screen.getByLabelText('Escalade'))
		const provider = panel.getByLabelText(
			"Fournisseur d'escalade",
		) as HTMLSelectElement
		expect(provider.value).toBe('')
		expect(
			within(provider)
				.getAllByRole('option')
				.map((option) => option.textContent),
		).toEqual(['Désactivée', 'Laya local', 'Jev local (désactivé)'])
		expect(
			(panel.getByLabelText('À partir de') as HTMLInputElement).value,
		).toBe('40')
		expect(
			(panel.getByLabelText("Jusqu'à (exclu)") as HTMLInputElement).value,
		).toBe('70')
		expect(panel.getByText(/L'escalade est désactivée/)).toBeTruthy()
	})

	it('edits the escalation provider and the grey band', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		const panel = within(screen.getByLabelText('Escalade'))
		fireEvent.change(panel.getByLabelText("Fournisseur d'escalade"), {
			target: { value: 'p-laya' },
		})
		expect(lastValue(seen).escalation).toMatchObject({ provider: 'p-laya' })
		fireEvent.change(panel.getByLabelText('À partir de'), {
			target: { value: '30' },
		})
		expect(lastValue(seen).escalation).toMatchObject({ band_low: 30 })
		fireEvent.change(panel.getByLabelText("Jusqu'à (exclu)"), {
			target: { value: '' },
		})
		expect(lastValue(seen).escalation).toMatchObject({ band_high: null })
		fireEvent.change(panel.getByLabelText("Fournisseur d'escalade"), {
			target: { value: '' },
		})
		expect(lastValue(seen).escalation).toMatchObject({ provider: null })
	})

	it('flags an unordered grey band', () => {
		const initial = {
			...base(),
			escalation: { provider: 'p-laya', band_low: 70, band_high: 40 },
		}
		render(<Harness initial={initial} />)
		const panel = within(screen.getByLabelText('Escalade'))
		expect(
			panel.getByText("« Jusqu'à » doit être supérieur à « À partir de »"),
		).toBeTruthy()
		expect(
			panel.getByLabelText("Jusqu'à (exclu)").getAttribute('aria-invalid'),
		).toBe('true')
		expect(
			panel.getByText(/Escalade des leads dont la confiance est de 70 %/),
		).toBeTruthy()
	})

	it('shows the axis identifier next to its intitulé and edits it', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const axis = within(sheet().getByLabelText('Axe 1'))
		const id = axis.getByLabelText("Identifiant de l'axe") as HTMLInputElement
		expect(id.value).toBe('pertinence')
		expect(id.className).toContain('font-mono')
		expect(id.getAttribute('type')).not.toBe('hidden')
		expect(axis.getByLabelText("Intitulé de l'axe")).toBeTruthy()
		fireEvent.change(id, { target: { value: 'fit' } })
		expect(questionsOf(qualificationList(lastValue(seen))[0])[0]).toMatchObject(
			{ id: 'fit' },
		)
	})

	it('lays the options out as one table with a single header row', () => {
		render(<Harness />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const axis = within(sheet().getByLabelText('Axe 1'))
		const table = axis.getByRole('table')
		const headers = within(table)
			.getAllByRole('columnheader')
			.map((header) => header.textContent)
		expect(headers).toEqual([
			'Valeur',
			'Libellé',
			'Score',
			'Exclusion',
			'Actions',
		])
		expect(axis.getAllByText('Valeur')).toHaveLength(1)
		expect(axis.getAllByText('Score')).toHaveLength(1)
		const rows = within(table).getAllByRole('row')
		expect(rows).toHaveLength(5)
		expect(within(rows[1] as HTMLElement).getAllByRole('textbox')).toHaveLength(
			2,
		)
		expect(within(rows[1] as HTMLElement).getByRole('spinbutton')).toBeTruthy()
	})

	it('pins the unknown option last, labelled and impossible to delete', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const axis = within(sheet().getByLabelText('Axe 1'))
		const rows = within(axis.getByRole('table')).getAllByRole('row')
		const last = rows[rows.length - 1] as HTMLElement
		expect(last.getAttribute('data-unknown')).toBe('true')
		expect(within(last).getByText('Option « inconnu »')).toBeTruthy()
		expect(within(last).queryByLabelText(/^Valeur de l'option/)).toBeNull()
		const remove = within(last).getByRole('button', {
			name: /^Supprimer l'option 4/,
		}) as HTMLButtonElement
		expect(remove.disabled).toBe(true)
		fireEvent.click(remove)
		expect(seen).not.toHaveBeenCalled()
		expect(within(axis.getByRole('table')).getAllByRole('row')).toHaveLength(5)
		expect(
			rows.filter((row) => row.getAttribute('data-unknown') === 'true'),
		).toHaveLength(1)
	})

	it('keeps the unknown option last when an option is added and edits its score', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		fireEvent.click(
			sheet().getByRole('button', { name: "Ajouter une option à l'axe 1" }),
		)
		const axis = within(sheet().getByLabelText('Axe 1'))
		const rows = within(axis.getByRole('table')).getAllByRole('row')
		const last = rows[rows.length - 1] as HTMLElement
		expect(last.getAttribute('data-unknown')).toBe('true')
		expect(rows).toHaveLength(6)
		const score = within(last).getByRole('spinbutton') as HTMLInputElement
		expect(score.value).toBe('0.5')
		fireEvent.change(score, { target: { value: '0.2' } })
		const unknown = optionsOf(
			questionsOf(qualificationList(lastValue(seen))[0])[0],
		).find((item) => isUnknownOption(item))
		expect(unknown).toMatchObject({ value: 'inconnu', score: 0.2 })
	})

	it('adds a second axis with its own table', () => {
		render(<Harness />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		fireEvent.click(
			sheet().getByRole('button', { name: "Ajouter un axe d'analyse" }),
		)
		expect(sheet().getAllByRole('table')).toHaveLength(2)
		expect(sheet().getByLabelText('Axe 2')).toBeTruthy()
	})

	it('shows the description on the agent card', () => {
		render(<Harness />)
		expect(
			screen.getByText("La pertinence du lead pour l'offre du profil."),
		).toBeTruthy()
	})

	it('previews the summary field of the contract', () => {
		render(<Harness />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		fireEvent.click(
			sheet().getByRole('button', { name: 'Aperçu du JSON contractuel' }),
		)
		const parsed = JSON.parse(
			screen.getByTestId('contract-preview').textContent ?? '',
		)
		expect(parsed.properties.summary.maxLength).toBe(600)
		expect(parsed.required).toEqual(['summary', 'answers'])
	})

	it('edits axes and options', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		fireEvent.click(
			sheet().getByRole('button', { name: "Ajouter un axe d'analyse" }),
		)
		let question = within(sheet().getByLabelText('Axe 2'))
		fireEvent.change(question.getByLabelText("Intitulé de l'axe"), {
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
				name: "Ajouter une option à l'axe 2",
			}),
		)
		question = within(sheet().getByLabelText('Axe 2'))
		expect(
			optionsOf(questionsOf(qualificationList(lastValue(seen))[0])[1]),
		).toHaveLength(4)
		const scores = question.getAllByLabelText(/^Score de l'option/)
		fireEvent.change(scores[0] as HTMLElement, { target: { value: '0.7' } })
		fireEvent.change(
			question.getAllByLabelText(/^Valeur de l'option/)[0] as HTMLElement,
			{
				target: { value: 'eleve' },
			},
		)
		fireEvent.change(
			question.getAllByLabelText(/^Libellé de l'option/)[0] as HTMLElement,
			{ target: { value: 'Élevé' } },
		)
		const options = optionsOf(
			questionsOf(qualificationList(lastValue(seen))[0])[1],
		)
		expect(options[0]).toEqual({
			value: 'eleve',
			label: 'Élevé',
			score: 0.7,
			is_unknown: false,
			excludes: false,
		})
		fireEvent.click(
			question.getByRole('button', {
				name: "Supprimer l'option 3 de l'axe 2",
			}),
		)
		expect(
			optionsOf(questionsOf(qualificationList(lastValue(seen))[0])[1]),
		).toHaveLength(3)
		fireEvent.click(sheet().getByRole('button', { name: "Supprimer l'axe 2" }))
		expect(questionsOf(qualificationList(lastValue(seen))[0])).toHaveLength(1)
	})

	it('score inputs step by a tenth between 0 and 1', () => {
		render(<Harness />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const score = sheet().getAllByLabelText(
			/^Score de l'option/,
		)[0] as HTMLInputElement
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
					name: "Supprimer l'option 1 de l'axe 1",
				}) as HTMLButtonElement
			).disabled,
		).toBe(true)
		expect(
			(
				sheet().getByRole('button', {
					name: "Supprimer l'axe 1",
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
		expect(parsed.required).toEqual(['summary', 'answers'])
		expect(parsed.properties.summary).toEqual({
			type: 'string',
			minLength: 1,
			maxLength: 600,
		})
		expect(parsed.properties.answers.required).toEqual(['pertinence'])
		expect(
			parsed.properties.answers.properties.pertinence.properties.choice.enum,
		).toEqual(['forte', 'moyenne', 'faible', 'inconnu'])
		const evidence =
			parsed.properties.answers.properties.pertinence.properties.evidence
		expect(evidence.type).toBe('string')
		expect(evidence.maxLength).toBe(300)
		expect(parsed.properties.answers.properties.pertinence.required).toContain(
			'evidence',
		)
		fireEvent.click(
			sheet().getByRole('button', {
				name: "Ajouter une option à l'axe 1",
			}),
		)
		expect(
			JSON.parse(screen.getByTestId('contract-preview').textContent ?? '')
				.properties.answers.properties.pertinence.properties.probabilities
				.required,
		).toEqual(['forte', 'moyenne', 'faible', 'option-5', 'inconnu'])
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

describe('evidence settings panel', () => {
	it('is titled verdicts and evidence and shows the defaults', () => {
		render(<Harness />)
		expect(
			screen.getByRole('heading', { name: 'Verdicts et preuves' }),
		).toBeTruthy()
		expect(screen.getByLabelText('Preuves minimales')).toHaveProperty(
			'value',
			'50',
		)
		expect(screen.getByLabelText('Plancher de calibration')).toHaveProperty(
			'value',
			'5',
		)
		expect(screen.getByLabelText('Influence des preuves')).toHaveProperty(
			'value',
			'60',
		)
		expect(screen.queryByLabelText('Score de référence')).toBeNull()
		for (const label of [
			'Poids : volume de signaux',
			'Poids : diversité des sources',
			'Poids : environnement',
			'Poids : force des signaux',
		]) {
			expect(screen.getByLabelText(label)).toHaveProperty('value', '1')
		}
	})

	it('writes percentages as fractions and keeps the thresholds', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.change(screen.getByLabelText('Preuves minimales'), {
			target: { value: '65' },
		})
		expect(lastValue(seen).verdict).toEqual({
			reliable_from: 70,
			discard_below: 30,
			min_evidence: 0.65,
		})
		fireEvent.change(screen.getByLabelText('Plancher de calibration'), {
			target: { value: '10' },
		})
		expect(lastValue(seen).verdict).toMatchObject({ calibration_floor: 0.1 })
		fireEvent.change(screen.getByLabelText('Influence des preuves'), {
			target: { value: '40' },
		})
		expect(lastValue(seen).evidence).toEqual({ influence: 0.4 })
	})

	it('edits the weights of the evidence components', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.change(screen.getByLabelText('Poids : environnement'), {
			target: { value: '3' },
		})
		expect(lastValue(seen).evidence).toEqual({
			weights: { environment: 3 },
		})
	})

	it('shows the half-life of every signal kind in days with its default', () => {
		render(<Harness />)
		const group = screen.getByRole('group', {
			name: 'Demi-vie des signaux (jours)',
		})
		const expected: Record<string, string> = {
			"Offre d'emploi": '90',
			"Appel d'offres": '180',
			'Marché attribué': '365',
			'Fournisseur SaaS': '365',
			'Produit détecté': '365',
			'Marqueur de dette technique': '365',
			'Contact par rôle trouvé': '365',
			'Autres signaux': '180',
		}
		for (const [label, days] of Object.entries(expected)) {
			expect(within(group).getByLabelText(label)).toHaveProperty('value', days)
		}
	})

	it('writes an edited half-life into the evidence section', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.change(screen.getByLabelText("Offre d'emploi"), {
			target: { value: '45' },
		})
		expect(lastValue(seen).evidence).toEqual({
			half_life_days: { job_offer: 45 },
		})
	})

	it('rejects a half-life outside 1 to 3650 days', () => {
		render(<Harness />)
		fireEvent.change(screen.getByLabelText("Appel d'offres"), {
			target: { value: '4000' },
		})
		expect(screen.getByRole('alert').textContent).toContain('1 à 3650 jours')
		expect(
			screen.getByLabelText("Appel d'offres").getAttribute('aria-invalid'),
		).toBe('true')
	})

	it('rejects a calibration floor above 20 %', () => {
		render(<Harness />)
		fireEvent.change(screen.getByLabelText('Plancher de calibration'), {
			target: { value: '25' },
		})
		expect(screen.getByRole('alert').textContent).toContain('0 à 20')
		expect(
			screen
				.getByLabelText('Plancher de calibration')
				.getAttribute('aria-invalid'),
		).toBe('true')
	})

	it('rejects weights that are all zero', () => {
		render(<Harness />)
		for (const label of [
			'Poids : volume de signaux',
			'Poids : diversité des sources',
			'Poids : environnement',
			'Poids : force des signaux',
		]) {
			fireEvent.change(screen.getByLabelText(label), {
				target: { value: '0' },
			})
		}
		expect(screen.getByRole('alert').textContent).toContain('Au moins un poids')
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
		const scores = sheet().getAllByLabelText(/^Score de l'option/)
		expect(scores[1]?.getAttribute('aria-invalid')).toBe('true')
		expect(scores[0]?.getAttribute('aria-invalid')).toBe('false')
		expect(sheet().getByText('Doit être un nombre de 0 à 1')).toBeTruthy()
	})

	it('switches the learned calibration of a qualification agent', () => {
		const seen = vi.fn()
		render(<Harness onValue={seen} />)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		const toggle = sheet().getByLabelText('Calibration apprise')
		expect(toggle.getAttribute('aria-checked')).toBe('true')
		expect(
			sheet().getByText(
				"Corrige chaque jour les probabilités de cet agent d'après vos décisions, dès 50 leads étiquetés dont au moins 10 de chaque classe.",
			),
		).toBeTruthy()
		fireEvent.click(toggle)
		expect(toggle.getAttribute('aria-checked')).toBe('false')
		expect(
			(lastValue(seen).qualification_agents as JsonObject[])[0]
				?.learned_calibration,
		).toBe(false)
		expect(sheet().getByText('Désactivée')).toBeTruthy()
	})

	it('shows the state and the Brier scores of the agent when known', () => {
		render(
			<Harness
				calibration={{
					'qualification:pertinence': {
						status: 'insufficient',
						n: 12,
						required: 50,
						brierBefore: 0.31,
						brierAfter: null,
					},
				}}
			/>,
		)
		fireEvent.click(screen.getByRole('button', { name: 'Modifier Pertinence' }))
		expect(sheet().getByText('Pas assez de décisions (12/50)')).toBeTruthy()
		expect(sheet().getByText(/Score de Brier avant 0,31, après —/)).toBeTruthy()
	})
})
