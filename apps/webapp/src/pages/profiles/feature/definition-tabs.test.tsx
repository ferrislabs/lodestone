import {
	cleanup,
	fireEvent,
	render,
	screen,
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
import { summary } from '#/pages/source-providers/lib/fixtures'

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

const CATALOG = [summary({ key: 'boamp', name: 'BOAMP', status: 'ready' })]

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
						catalog={CATALOG}
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
	const dialog = within(screen.getByRole('dialog'))
	fireEvent.click(
		dialog.getByRole('radio', {
			name: (name) => name.startsWith(KIND_META[kind].label),
		}),
	)
	if (kind === 'search') {
		fireEvent.click(dialog.getByRole('button', { name: 'BOAMP' }))
	}
	fireEvent.click(dialog.getByRole('button', { name: 'Suivant' }))
	if (kind === 'search' || kind === 'certificates') {
		const tags = dialog.getByLabelText(
			kind === 'search' ? 'Mots-clés' : 'Libellés',
		)
		fireEvent.change(tags, { target: { value: 'sso' } })
		fireEvent.keyDown(tags, { key: 'Enter' })
	}
	if (kind === 'dns') {
		fireEvent.click(dialog.getByText('Ajouter un motif'))
		fireEvent.change(dialog.getByLabelText('Fournisseur'), {
			target: { value: 'Okta' },
		})
		fireEvent.change(dialog.getByLabelText('Suffixe cible'), {
			target: { value: 'okta.com' },
		})
	}
	fireEvent.click(dialog.getByRole('button', { name: 'Suivant' }))
	fireEvent.click(dialog.getByRole('button', { name: 'Créer' }))
}

function openAgent(position = 0) {
	fireEvent.click(
		screen.getAllByRole('button', { name: /Modifier/ })[
			position
		] as HTMLElement,
	)
}

function backToList() {
	fireEvent.click(screen.getByRole('button', { name: 'Agents de sourcing' }))
}

describe('definition tabs', () => {
	it('shows only the subtree of the selected tab', () => {
		render(<Harness />)
		expect(screen.getByLabelText(/Offre/)).toBeTruthy()
		expect(screen.getByLabelText('Langue des réponses')).toBeTruthy()
		expect(screen.queryByLabelText('Adresses à privilégier')).toBeNull()
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.scoring }))
		expect(
			screen.getByRole('button', { name: 'Ajouter un agent de scoring' }),
		).toBeTruthy()
		expect(screen.queryByLabelText(/Offre/)).toBeNull()
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.sourcing }))
		expect(
			screen.getByRole('button', { name: 'Ajouter un agent' }),
		).toBeTruthy()
	})

	it('badges the agents tab for a server problem inside an agent', () => {
		const first = newAgent('search', [])
		const second = newAgent('search', [first])
		render(
			<Harness
				initial={{ ...blank(), agents: [first, second] }}
				problems={[
					{
						path: 'agents[1].keywords[0]',
						message: 'Ne doit pas être vide',
					},
				]}
			/>,
		)
		const tab = screen.getByRole('tab', { name: /Sourcing/ })
		expect(within(tab).getByLabelText('1 problème')).toBeTruthy()
		expect(
			within(screen.getByRole('tab', { name: /Identité/ })).queryByRole('img'),
		).toBeNull()
	})

	it('shows the problem under the keyword field on the agents tab', () => {
		const agent = newAgent('search', [])
		render(
			<Harness
				startTab="sourcing"
				initial={{ ...blank(), agents: [{ ...agent, keywords: [''] }] }}
				problems={[
					{
						path: 'agents[0].keywords[0]',
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
		render(<Harness startTab="sourcing" />)
		expect(
			screen.getByText(
				'Aucun agent pour le moment. Ajouter un agent pour démarrer la recherche.',
			),
		).toBeTruthy()
		expect(screen.queryByRole('button', { name: 'Créer le profil' })).toBeNull()
		addAgent('search')
		expect(
			screen
				.getByRole('button', { name: 'Créer le profil' })
				.hasAttribute('disabled'),
		).toBe(false)
	})

	for (const kind of AGENT_KINDS) {
		it(`adds a ${kind} agent with only its fields`, () => {
			render(<Harness startTab="sourcing" />)
			addAgent(kind)
			openAgent()
			expect(screen.getByLabelText("Nom de l'agent")).toHaveProperty(
				'value',
				KIND_META[kind].label,
			)
			expect(
				screen.queryAllByText('Critères LLM propres à cet agent').length,
			).toBe(0)
			const has = (text: string) => screen.queryAllByText(text).length > 0
			if (kind === 'certificates') {
				expect(has('Libellés')).toBe(true)
				expect(has('Mots-clés')).toBe(false)
			} else if (kind === 'dns') {
				expect(has('Motifs CNAME')).toBe(true)
				expect(has('Libellés')).toBe(false)
			} else {
				expect(has('Mots-clés')).toBe(true)
				expect(has('Exclusions')).toBe(true)
				expect(has('Mots-clés distincts pour un signal fort')).toBe(true)
				expect(has('Empreintes')).toBe(false)
				expect(has('Libellés')).toBe(false)
			}
		})
	}

	it('suffixes ids of agents of the same kind', () => {
		render(<Harness startTab="sourcing" />)
		addAgent('search')
		addAgent('search')
		openAgent(1)
		expect(screen.getByLabelText('Identifiant')).toHaveProperty(
			'value',
			'recherche-2',
		)
		backToList()
		openAgent(0)
		expect(screen.getByLabelText('Identifiant')).toHaveProperty(
			'value',
			'recherche',
		)
	})

	it('lays agents out as grid cells with the add card last', () => {
		const first = newAgent('search', [])
		const second = newAgent('search', [first])
		const third = newAgent('search', [first, second])
		render(
			<Harness
				startTab="sourcing"
				initial={{ ...blank(), agents: [first, second, third] }}
			/>,
		)
		const grid = document.querySelector('[data-agent]')?.parentElement
		expect(grid?.className).toContain('xl:grid-cols-3')
		expect(grid?.querySelectorAll('[data-agent]')).toHaveLength(3)
		expect(grid?.lastElementChild?.hasAttribute('data-add-agent')).toBe(true)
		expect(screen.getAllByText('0 mot-clé')).toHaveLength(3)
		openAgent(1)
		expect(screen.getAllByLabelText('Identifiant')).toHaveLength(1)
		backToList()
		const cards = document.querySelectorAll<HTMLElement>('[data-agent]')
		expect(cards[1]?.className).toContain('ring-2')
		expect(cards[0]?.className).not.toContain('ring-2')
		expect(cards[2]?.className).not.toContain('ring-2')
		expect(cards[1]?.className).not.toContain('col-span')
	})

	it('follows the name with the id until the id is edited', () => {
		render(<Harness startTab="sourcing" />)
		addAgent('search')
		openAgent()
		fireEvent.change(screen.getByLabelText("Nom de l'agent"), {
			target: { value: 'Recherche anglais' },
		})
		expect(screen.getByLabelText('Identifiant')).toHaveProperty(
			'value',
			'recherche-anglais',
		)
		fireEvent.change(screen.getByLabelText('Identifiant'), {
			target: { value: 'mine' },
		})
		fireEvent.change(screen.getByLabelText("Nom de l'agent"), {
			target: { value: 'Z' },
		})
		expect(screen.getByLabelText('Identifiant')).toHaveProperty('value', 'mine')
	})

	it('removes an agent after an inline confirmation', () => {
		render(<Harness startTab="sourcing" />)
		addAgent('search')
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

	it('opens the agent page with the agent name and returns to the list', () => {
		const agent = { ...newAgent('search', []), name: 'ATS français' }
		render(
			<Harness startTab="sourcing" initial={{ ...blank(), agents: [agent] }} />,
		)
		fireEvent.click(
			screen.getByRole('button', { name: 'Modifier ATS français' }),
		)
		expect(screen.getByLabelText("Nom de l'agent")).toHaveProperty(
			'value',
			'ATS français',
		)
		expect(screen.getByText(KIND_META.search.label)).toBeTruthy()
		expect(screen.queryByRole('dialog')).toBeNull()
		backToList()
		expect(screen.queryByLabelText("Nom de l'agent")).toBeNull()
		expect(
			screen.getByRole('button', { name: 'Modifier ATS français' }),
		).toBeTruthy()
	})

	it('keeps the identifier behind a collapsed advanced section', () => {
		render(<Harness startTab="sourcing" />)
		addAgent('search')
		openAgent()
		const advanced = screen.getByText('Avancé').closest('details')
		expect(advanced?.hasAttribute('open')).toBe(false)
		expect(advanced?.contains(screen.getByLabelText('Identifiant'))).toBe(true)
	})

	it('lays a keyword agent out with a 1 to 5 choice and two tag inputs', () => {
		render(<Harness startTab="sourcing" />)
		addAgent('search')
		openAgent()
		const keywords = screen.getByLabelText('Mots-clés')
		const exclusions = screen.getByLabelText('Exclusions')
		expect(keywords.closest('.md\\:grid-cols-2')).toBe(
			exclusions.closest('.md\\:grid-cols-2'),
		)
		expect(keywords.closest('.md\\:grid-cols-2')).toBeTruthy()
		const choice = screen.getByRole('radiogroup', {
			name: 'Mots-clés distincts pour un signal fort',
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
		expect(screen.getByLabelText('Retirer cloud')).toBeTruthy()
		backToList()
		expect(screen.getByText('2 mots-clés')).toBeTruthy()
	})

	it('lays a dns agent out as compact rows', () => {
		render(<Harness startTab="sourcing" />)
		addAgent('dns')
		openAgent()
		const row = document.querySelector<HTMLElement>(
			'[data-row="agents[0].patterns[0]"]',
		) as HTMLElement
		expect(within(row).getByLabelText('Fournisseur')).toBeTruthy()
		expect(within(row).getByLabelText('Suffixe cible')).toBeTruthy()
		fireEvent.click(within(row).getByRole('button', { name: /Supprimer/ }))
		expect(
			document.querySelector('[data-row="agents[0].patterns[0]"]'),
		).toBeNull()
	})

	it('shows the save bar only when dirty and cancel resets after a confirmation', () => {
		render(<Harness startTab="sourcing" />)
		expect(screen.queryByTestId('save-bar')).toBeNull()
		addAgent('search')
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

	it('warns when a dns agent has no certificates agent', () => {
		render(<Harness startTab="sourcing" />)
		addAgent('dns')
		expect(screen.getByRole('alert').textContent).toContain(
			'Résout les hôtes trouvés par un agent Certificats',
		)
		addAgent('certificates')
		expect(screen.queryByRole('alert')).toBeNull()
		expect(
			screen.getByText(/Résout les hôtes trouvés par un agent Certificats/),
		).toBeTruthy()
	})

	it('has no Réglages tab', () => {
		render(<Harness />)
		expect(screen.queryByRole('tab', { name: 'Réglages' })).toBeNull()
		expect(
			screen.getAllByRole('tab').map((tab) => tab.textContent?.trim()),
		).toEqual([
			'Identité',
			'Sourcing',
			'Enrichissement',
			'Qualification',
			'Scoring',
		])
	})

	it('explains the catalog, the three agent types and the enrichment tab', () => {
		render(<Harness startTab="sourcing" />)
		const panel = screen.getByTestId('help-panel')
		const paragraphs = panel.querySelectorAll('p')
		expect(paragraphs[0]?.textContent).toBe(
			"Le catalogue liste les sources disponibles pour toute l'organisation ; un agent décide quoi chercher, et dans quelles sources.",
		)
		const help = within(panel)
		for (const kind of AGENT_KINDS) {
			expect(help.getByText(KIND_META[kind].label)).toBeTruthy()
		}
		expect(help.getByText(/onglet Enrichissement/)).toBeTruthy()
		expect(panel.textContent).not.toMatch(/Pages carrières|pages carrières/)
		expect(panel.textContent).not.toContain('Sonde HTTP ←')
	})

	it('edits the enrichment steps on their own tab', () => {
		render(<Harness />)
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.enrichment }))
		const probe = within(screen.getByRole('region', { name: 'Sonde HTTP' }))
		const jobs = within(
			screen.getByRole('region', {
				name: "Offres d'emploi de l'entreprise",
			}),
		)
		const dossier = within(
			screen.getByRole('region', { name: 'Dossier entreprise' }),
		)
		expect(probe.getByText('Empreintes')).toBeTruthy()
		expect(probe.getByText('Sous-domaines supplémentaires')).toBeTruthy()
		expect(probe.getByText('Produits exclus')).toBeTruthy()
		expect(probe.getByLabelText('Retirer auth')).toBeTruthy()
		const probeSwitch = probe.getByRole('switch', {
			name: 'Activer la sonde HTTP',
		})
		expect(probeSwitch.getAttribute('aria-checked')).toBe('false')
		fireEvent.click(probeSwitch)
		expect(probeSwitch.getAttribute('aria-checked')).toBe('true')
		fireEvent.click(probe.getByText('Ajouter une empreinte'))
		const row = document.querySelector<HTMLElement>(
			'[data-row="enrichment.http_probe.fingerprints[0]"]',
		) as HTMLElement
		expect(within(row).getByText(/^Chemin/)).toBeTruthy()
		expect(
			jobs.getByText(/Vide : les mots-clés des agents de recherche/),
		).toBeTruthy()
		expect(jobs.getByText('Avancé')).toBeTruthy()
		expect(
			jobs.getByText('Mots-clés distincts pour un signal fort'),
		).toBeTruthy()
		fireEvent.click(
			jobs.getByRole('switch', {
				name: "Activer les offres d'emploi de l'entreprise",
			}),
		)
		expect(
			jobs
				.getByRole('switch', {
					name: "Activer les offres d'emploi de l'entreprise",
				})
				.getAttribute('aria-checked'),
		).toBe('true')
		expect(dossier.getByText(/Toujours actif/)).toBeTruthy()
		expect(dossier.queryByRole('switch')).toBeNull()
		const help = within(screen.getByTestId('help-panel'))
		expect(help.getByText(/ne crée jamais de lead/)).toBeTruthy()
		expect(help.getByText(/30 minutes/)).toBeTruthy()
		expect(
			screen.queryByRole('button', { name: 'Ajouter un agent' }),
		).toBeNull()
	})

	it('shows the search keywords used when the job keywords are empty', () => {
		const agent = { ...newAgent('search', [], ['boamp']), keywords: ['sso'] }
		render(
			<Harness
				startTab="enrichment"
				initial={{ ...blank(), agents: [agent] }}
			/>,
		)
		expect(screen.getByText("Utilisés aujourd'hui : sso")).toBeTruthy()
	})

	it('badges the enrichment tab for a problem inside an enrichment step', () => {
		render(
			<Harness
				problems={[
					{
						path: 'enrichment.http_probe.fingerprints',
						message: 'Au moins une empreinte',
					},
				]}
			/>,
		)
		const tab = screen.getByRole('tab', { name: /Enrichissement/ })
		expect(within(tab).getByLabelText('1 problème')).toBeTruthy()
		fireEvent.click(tab)
		expect(screen.getByText('Au moins une empreinte')).toBeTruthy()
	})

	it('offers the LLM answer language as a french select in the identity tab', () => {
		render(<Harness />)
		const select = screen.getByLabelText('Langue des réponses')
		expect(select).toHaveProperty('value', 'fr')
		expect(
			within(select)
				.getAllByRole('option')
				.map((option) => option.textContent),
		).toEqual(['Français', 'English'])
		fireEvent.change(select, { target: { value: 'en' } })
		expect(select).toHaveProperty('value', 'en')
	})

	it('puts the contact ranking in a Contacts section at the bottom of the sourcing tab', () => {
		render(<Harness startTab="sourcing" />)
		expect(screen.getByRole('heading', { name: 'Contacts' })).toBeTruthy()
		expect(screen.getByText('Adresses à privilégier')).toBeTruthy()
		expect(screen.getByText('Adresses à deviner')).toBeTruthy()
		expect(
			screen.getByText(/la partie avant le @, par exemple security ou contact/),
		).toBeTruthy()
		const add = screen.getByRole('button', { name: 'Ajouter un agent' })
		const contacts = screen.getByRole('heading', { name: 'Contacts' })
		expect(
			add.compareDocumentPosition(contacts) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy()
	})

	it('keeps no calibration or threshold field in any tab', () => {
		render(<Harness />)
		for (const tab of [
			'identity',
			'sourcing',
			'enrichment',
			'qualification',
			'scoring',
		] as const) {
			fireEvent.click(
				screen.getByRole('tab', { name: new RegExp(TAB_LABELS[tab]) }),
			)
			expect(screen.queryByLabelText('Seuil de qualification')).toBeNull()
			expect(screen.queryByLabelText(/Critères de pertinence/)).toBeNull()
			expect(screen.queryByLabelText('Calibration apprise')).toBeNull()
		}
	})

	it('never renders a JSON editor', () => {
		const tabs: EditorTab[] = [
			'identity',
			'sourcing',
			'enrichment',
			'qualification',
			'scoring',
		]
		render(
			<Harness
				startTab="sourcing"
				initial={{ ...blank(), agents: [newAgent('dns', [])] }}
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
