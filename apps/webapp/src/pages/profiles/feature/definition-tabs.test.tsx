import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from '@testing-library/react'
import { useState } from 'react'
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'

import { defaultValue, type JsonObject, toJsonObject } from '#/lib/schema-form'
import { DefinitionTabContent } from '#/pages/profiles/feature/definition-form'
import { useDefinitionEditor } from '#/pages/profiles/feature/use-definition-editor'
import {
	AGENT_KINDS,
	type AgentKind,
	agentList,
	KIND_META,
	newAgent,
} from '#/pages/profiles/lib/agents'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import {
	type EditorTab,
	editorTabItems,
	TAB_LABELS,
} from '#/pages/profiles/lib/tabs'
import type { Problem } from '#/pages/profiles/lib/types'
import { ProfileEditor } from '#/pages/profiles/ui/profile-editor'
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

function blank(): JsonObject {
	return toJsonObject(
		defaultValue(PROFILE_SCHEMA, PROFILE_SCHEMA, PROFILE_HINTS),
	)
}

function Harness({
	initial = blank(),
	problems = [],
	startTab = 'identity',
}: {
	initial?: JsonObject
	problems?: readonly Problem[]
	startTab?: EditorTab
}) {
	const [seed] = useState(initial)
	const editor = useDefinitionEditor(seed)
	const value = editor.value
	const setValue = editor.set
	const [tab, setTab] = useState<EditorTab>(startTab)
	return (
		<>
			<TabBar tabs={editorTabItems(problems)} active={tab} onChange={setTab} />
			<ProfileEditor
				form={
					<DefinitionTabContent
						tab={tab}
						value={value}
						problems={problems}
						onChange={setValue}
					/>
				}
				problems={[]}
				notice={null}
				dirty={editor.dirty}
				saving={false}
				saveLabel="Créer le profil"
				confirmingReset={editor.confirmingReset}
				onRequestReset={editor.requestReset}
				onCancelReset={editor.cancelReset}
				onConfirmReset={editor.reset}
				blockedReason={
					agentList(value).length === 0 ? 'Ajouter au moins un agent.' : null
				}
				onSave={() => undefined}
			/>
		</>
	)
}

function addAgent(kind: AgentKind) {
	fireEvent.click(screen.getByRole('button', { name: 'Ajouter un agent' }))
	fireEvent.click(
		within(screen.getByLabelText('Choisir une source')).getByText(
			KIND_META[kind].label,
		),
	)
}

function closeSheet() {
	fireEvent.keyDown(document, { key: 'Escape' })
}

function sheet() {
	return within(screen.getByRole('dialog'))
}

describe('definition tabs', () => {
	it('shows only the subtree of the selected tab', () => {
		render(<Harness />)
		expect(screen.getByLabelText(/Offre/)).toBeTruthy()
		expect(screen.queryByLabelText(/Seuil de qualification/)).toBeNull()
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.settings }))
		expect(screen.getByLabelText(/Seuil de qualification/)).toBeTruthy()
		expect(screen.queryByLabelText(/Offre/)).toBeNull()
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.scoring }))
		expect(
			screen.getByRole('button', { name: 'Ajouter un facteur' }),
		).toBeTruthy()
		expect(screen.queryByLabelText(/Seuil de qualification/)).toBeNull()
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.agents }))
		expect(
			screen.getByRole('button', { name: 'Ajouter un agent' }),
		).toBeTruthy()
	})

	it('badges the agents tab for a server problem inside an agent', () => {
		const first = newAgent('ats', [])
		const second = newAgent('ats', [first])
		render(
			<Harness
				initial={{ ...blank(), agents: [first, second] }}
				problems={[
					{
						path: 'agents[1].source.keywords[0]',
						message: 'Ne doit pas être vide',
					},
				]}
			/>,
		)
		const tab = screen.getByRole('tab', { name: /Agents/ })
		expect(within(tab).getByLabelText('1 problème')).toBeTruthy()
		expect(
			within(screen.getByRole('tab', { name: /Identité/ })).queryByRole('img'),
		).toBeNull()
	})

	it('shows the problem under the keyword field on the agents tab', () => {
		const agent = newAgent('ats', [])
		const source = { ...(agent.source as JsonObject), keywords: [''] }
		render(
			<Harness
				startTab="agents"
				initial={{ ...blank(), agents: [{ ...agent, source }] }}
				problems={[
					{
						path: 'agents[0].source.keywords[0]',
						message: 'Ne doit pas être vide',
					},
				]}
			/>,
		)
		expect(screen.queryByText('Ne doit pas être vide')).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: /Modifier/ }))
		expect(screen.getByText('Ne doit pas être vide')).toBeTruthy()
		expect(
			document.querySelector('[data-tag="0"]')?.getAttribute('aria-invalid'),
		).toBe('true')
	})

	it('shows the empty state and blocks creation without agents', () => {
		render(<Harness startTab="agents" />)
		expect(
			screen.getByText(
				'Aucun agent pour le moment. Ajouter un agent pour démarrer la recherche.',
			),
		).toBeTruthy()
		expect(screen.queryByRole('button', { name: 'Créer le profil' })).toBeNull()
		addAgent('boamp')
		closeSheet()
		expect(
			screen
				.getByRole('button', { name: 'Créer le profil' })
				.hasAttribute('disabled'),
		).toBe(false)
	})

	for (const kind of AGENT_KINDS) {
		it(`adds a ${kind} agent with only its fields`, async () => {
			render(<Harness startTab="agents" />)
			addAgent(kind)
			const scoped = sheet()
			expect(scoped.getByLabelText('Nom')).toHaveProperty(
				'value',
				KIND_META[kind].label,
			)
			await waitFor(() =>
				expect(document.activeElement).toBe(scoped.getByLabelText('Nom')),
			)
			expect(
				scoped.queryAllByText('Critères LLM propres à cet agent').length,
			).toBe(KIND_META[kind].triaged ? 1 : 0)
			const has = (text: string) => scoped.queryAllByText(text).length > 0
			if (kind === 'certificate_transparency') {
				expect(has('Libellés')).toBe(true)
				expect(has('Mots-clés')).toBe(false)
			} else if (kind === 'dns_cname') {
				expect(has('Motifs CNAME')).toBe(true)
				expect(has('Libellés')).toBe(false)
			} else if (kind === 'http_probe') {
				expect(has('Empreintes')).toBe(true)
				expect(has('Produits exclus')).toBe(true)
				expect(has('Mots-clés')).toBe(false)
			} else {
				expect(has('Mots-clés')).toBe(true)
				expect(has('Exclusions')).toBe(true)
				expect(has('Mots-clés distincts pour accepter sans LLM')).toBe(true)
				expect(has('Empreintes')).toBe(false)
				expect(has('Libellés')).toBe(false)
			}
		})
	}

	it('suffixes ids of agents of the same kind', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		closeSheet()
		addAgent('ats')
		expect(sheet().getByLabelText('Identifiant')).toHaveProperty(
			'value',
			'ats-2',
		)
		closeSheet()
		fireEvent.click(
			screen.getAllByRole('button', { name: /Modifier/ })[0] as HTMLElement,
		)
		expect(sheet().getByLabelText('Identifiant')).toHaveProperty('value', 'ats')
	})

	it('lays agents out as grid cells with the add card last', () => {
		const first = newAgent('ats', [])
		const second = newAgent('ats', [first])
		const third = newAgent('ats', [first, second])
		render(
			<Harness
				startTab="agents"
				initial={{ ...blank(), agents: [first, second, third] }}
			/>,
		)
		const grid = document.querySelector('[data-agent]')?.parentElement
		expect(grid?.className).toContain('xl:grid-cols-3')
		expect(grid?.querySelectorAll('[data-agent]')).toHaveLength(3)
		expect(grid?.lastElementChild?.hasAttribute('data-add-agent')).toBe(true)
		expect(screen.getAllByText('0 mot-clé')).toHaveLength(3)
		fireEvent.click(
			screen.getAllByRole('button', { name: /Modifier/ })[1] as HTMLElement,
		)
		const cards = grid?.querySelectorAll<HTMLElement>('[data-agent]') ?? []
		expect(cards[1]?.className).toContain('ring-2')
		expect(cards[0]?.className).not.toContain('ring-2')
		expect(cards[2]?.className).not.toContain('ring-2')
		expect(cards[1]?.className).not.toContain('col-span')
		expect(screen.getAllByLabelText('Identifiant')).toHaveLength(1)
	})

	it('follows the name with the id until the id is edited', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		fireEvent.change(screen.getByLabelText('Nom'), {
			target: { value: 'ATS anglais' },
		})
		expect(screen.getByLabelText('Identifiant')).toHaveProperty(
			'value',
			'ats-anglais',
		)
		fireEvent.change(screen.getByLabelText('Identifiant'), {
			target: { value: 'mine' },
		})
		fireEvent.change(screen.getByLabelText('Nom'), { target: { value: 'Z' } })
		expect(screen.getByLabelText('Identifiant')).toHaveProperty('value', 'mine')
	})

	it('removes an agent after an inline confirmation', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		closeSheet()
		fireEvent.click(screen.getByRole('button', { name: /Supprimer l'agent/ }))
		fireEvent.click(screen.getByRole('button', { name: 'Conserver' }))
		expect(document.querySelectorAll('[data-agent]')).toHaveLength(1)
		fireEvent.click(screen.getByRole('button', { name: /Supprimer l'agent/ }))
		fireEvent.click(
			screen.getByRole('button', { name: 'Confirmer la suppression' }),
		)
		expect(document.querySelectorAll('[data-agent]')).toHaveLength(0)
		expect(screen.getByText(/Aucun agent pour le moment/)).toBeTruthy()
	})

	it('opens the sheet with the agent name and closes it on Escape or Fermer', () => {
		const agent = { ...newAgent('ats', []), name: 'ATS français' }
		render(
			<Harness startTab="agents" initial={{ ...blank(), agents: [agent] }} />,
		)
		expect(screen.queryByRole('dialog')).toBeNull()
		fireEvent.click(
			screen.getByRole('button', { name: 'Modifier ATS français' }),
		)
		const dialog = screen.getByRole('dialog')
		expect(within(dialog).getByRole('heading').textContent).toContain(
			'ATS français',
		)
		expect(within(dialog).getByRole('heading').textContent).toContain(
			KIND_META.ats.label,
		)
		expect(dialog.className).toContain('sm:max-w-2xl')
		closeSheet()
		expect(screen.queryByRole('dialog')).toBeNull()
		fireEvent.click(
			screen.getByRole('button', { name: 'Modifier ATS français' }),
		)
		const close = within(screen.getByRole('dialog')).getAllByRole('button', {
			name: 'Fermer',
		})
		fireEvent.click(close[close.length - 1] as HTMLElement)
		expect(screen.queryByRole('dialog')).toBeNull()
	})

	it('keeps the identifier behind a collapsed advanced section', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		const advanced = sheet().getByText('Avancé').closest('details')
		expect(advanced?.hasAttribute('open')).toBe(false)
		expect(advanced?.contains(sheet().getByLabelText('Identifiant'))).toBe(true)
	})

	it('lays a keyword agent out with a 1 to 5 choice and two tag inputs', () => {
		render(<Harness startTab="agents" />)
		addAgent('boamp')
		const scoped = sheet()
		const keywords = scoped.getByLabelText('Mots-clés')
		const exclusions = scoped.getByLabelText('Exclusions')
		expect(keywords.closest('.md\\:grid-cols-2')).toBe(
			exclusions.closest('.md\\:grid-cols-2'),
		)
		expect(keywords.closest('.md\\:grid-cols-2')).toBeTruthy()
		const choice = scoped.getByRole('radiogroup', {
			name: 'Mots-clés distincts pour accepter sans LLM',
		})
		const radios = within(choice).getAllByRole('radio')
		expect(radios.map((radio) => radio.textContent)).toEqual([
			'1',
			'2',
			'3',
			'4',
			'5',
		])
		expect(radios[1]?.getAttribute('aria-checked')).toBe('true')
		fireEvent.click(radios[3] as HTMLElement)
		expect(radios[3]?.getAttribute('aria-checked')).toBe('true')
		expect(radios[1]?.getAttribute('aria-checked')).toBe('false')
		fireEvent.change(keywords, { target: { value: 'cloud' } })
		fireEvent.keyDown(keywords, { key: 'Enter' })
		expect(scoped.getByLabelText('Retirer cloud')).toBeTruthy()
		closeSheet()
		expect(screen.getByText('1 mot-clé')).toBeTruthy()
	})

	it('lays a dns_cname agent out as compact rows', () => {
		render(<Harness startTab="agents" />)
		addAgent('dns_cname')
		const scoped = sheet()
		fireEvent.click(scoped.getByText('Ajouter un motif'))
		const row = document.querySelector<HTMLElement>(
			'[data-row="agents[0].source.patterns[0]"]',
		) as HTMLElement
		expect(within(row).getByLabelText('Fournisseur')).toBeTruthy()
		expect(within(row).getByLabelText('Suffixe cible')).toBeTruthy()
		fireEvent.click(within(row).getByRole('button', { name: /Supprimer/ }))
		expect(
			document.querySelector('[data-row="agents[0].source.patterns[0]"]'),
		).toBeNull()
	})

	it('switches the variant of an http probe matcher inline', () => {
		render(<Harness startTab="agents" />)
		addAgent('http_probe')
		const scoped = sheet()
		fireEvent.click(scoped.getByLabelText('Ajouter : Empreintes'))
		expect(scoped.getByText('Version et dette technique')).toBeTruthy()
		fireEvent.click(scoped.getByText('Ajouter une condition'))
		const select = scoped.getByLabelText(
			'Type : Condition',
		) as HTMLSelectElement
		expect([...select.options].map((option) => option.textContent)).toEqual([
			'Statut',
			'En-tête',
			'Corps',
		])
		expect(scoped.getByLabelText('Code de statut attendu')).toBeTruthy()
		fireEvent.change(select, { target: { value: '1' } })
		expect(scoped.getByLabelText("Nom de l'en-tête")).toBeTruthy()
		expect(scoped.getByLabelText('Expression régulière')).toBeTruthy()
		expect(scoped.queryByLabelText('Code de statut attendu')).toBeNull()
		fireEvent.change(select, { target: { value: '2' } })
		expect(scoped.queryByLabelText("Nom de l'en-tête")).toBeNull()
		expect(scoped.getByLabelText('Expression régulière')).toBeTruthy()
		expect(scoped.getByLabelText('Produits exclus')).toBeTruthy()
	})

	it('shows the save bar only when dirty and cancel resets after a confirmation', () => {
		render(<Harness startTab="agents" />)
		expect(screen.queryByTestId('save-bar')).toBeNull()
		addAgent('ats')
		closeSheet()
		const bar = within(screen.getByTestId('save-bar'))
		expect(bar.getByText('Modifications non enregistrées')).toBeTruthy()
		expect(bar.getByRole('button', { name: 'Créer le profil' })).toBeTruthy()
		expect(
			screen.getAllByRole('button', { name: 'Créer le profil' }),
		).toHaveLength(1)
		fireEvent.click(
			bar.getByRole('button', { name: 'Annuler les modifications' }),
		)
		fireEvent.click(
			bar.getByRole('button', { name: 'Conserver mes modifications' }),
		)
		expect(screen.getByTestId('save-bar')).toBeTruthy()
		fireEvent.click(
			bar.getByRole('button', { name: 'Annuler les modifications' }),
		)
		fireEvent.click(bar.getByRole('button', { name: "Confirmer l'annulation" }))
		expect(screen.queryByTestId('save-bar')).toBeNull()
		expect(screen.getByText(/Aucun agent pour le moment/)).toBeTruthy()
	})

	it('warns when an http probe has no certificate transparency agent', () => {
		render(<Harness startTab="agents" />)
		addAgent('http_probe')
		expect(screen.getByRole('alert').textContent).toContain(
			'Sonde les hôtes trouvés par un agent Transparence des certificats',
		)
		closeSheet()
		addAgent('certificate_transparency')
		expect(screen.queryByRole('alert')).toBeNull()
		expect(
			screen.getByText(
				/Sonde les hôtes trouvés par un agent Transparence des certificats/,
			),
		).toBeTruthy()
	})

	it('stores agent criteria and drops them when emptied', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		const field = screen.getByLabelText('Critères LLM propres à cet agent')
		fireEvent.change(field, { target: { value: 'only devs' } })
		expect(field).toHaveProperty('value', 'only devs')
		fireEvent.change(field, { target: { value: '' } })
		expect(field).toHaveProperty('value', '')
	})

	it('offers the LLM answer language as a french select', () => {
		render(<Harness startTab="settings" />)
		const select = screen.getByLabelText('Langue des réponses du LLM')
		expect(select).toHaveProperty('value', 'fr')
		expect(
			within(select)
				.getAllByRole('option')
				.map((option) => option.textContent),
		).toEqual(['Français', 'English'])
		fireEvent.change(select, { target: { value: 'en' } })
		expect(select).toHaveProperty('value', 'en')
	})

	it('never renders a JSON editor', () => {
		const tabs: EditorTab[] = ['identity', 'agents', 'scoring', 'settings']
		render(
			<Harness
				startTab="agents"
				initial={{ ...blank(), agents: [newAgent('http_probe', [])] }}
			/>,
		)
		for (const tab of tabs) {
			fireEvent.click(
				screen.getByRole('tab', { name: new RegExp(TAB_LABELS[tab]) }),
			)
			for (const box of screen.queryAllByRole('textbox')) {
				expect((box as HTMLInputElement).value).not.toContain('{')
			}
		}
	})
})
