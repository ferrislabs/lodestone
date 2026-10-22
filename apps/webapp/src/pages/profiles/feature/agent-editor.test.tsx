import {
	cleanup,
	fireEvent,
	render,
	screen,
	within,
} from '@testing-library/react'
import { useState } from 'react'
import {
	afterEach,
	beforeAll,
	beforeEach,
	describe,
	expect,
	it,
	vi,
} from 'vitest'

import { defaultValue, type JsonObject, toJsonObject } from '#/lib/schema-form'
import { DefinitionTabContent } from '#/pages/profiles/feature/definition-form'
import { useDefinitionEditor } from '#/pages/profiles/feature/use-definition-editor'
import { KIND_META, newAgent } from '#/pages/profiles/lib/agents'
import { COUNTRY_CODES, countryName } from '#/pages/profiles/lib/countries'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import {
	asEditorTab,
	editorTabItems,
	type ProfileTab,
	TAB_LABELS,
} from '#/pages/profiles/lib/tabs'
import { ProfileDetail } from '#/pages/profiles/ui/profile-detail'
import { ProfileEditor } from '#/pages/profiles/ui/profile-editor'
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

const CATALOG = [
	summary({ key: 'boamp', name: 'BOAMP', status: 'ready' }),
	summary({
		key: 'france_travail',
		name: 'France Travail',
		status: 'not_configured',
		capabilities: {
			keyword_search: true,
			filters: { remote: false, countries: false, contract: true },
		},
	}),
	summary({
		key: 'ted',
		name: 'TED',
		status: 'ready',
		capabilities: {
			keyword_search: true,
			filters: { remote: false, countries: true, contract: false },
		},
	}),
	summary({
		key: 'ats',
		name: "Offres d'emploi de l'entreprise",
		category: 'companies',
		status: 'ready',
		capabilities: {
			keyword_search: false,
			filters: { remote: false, countries: false, contract: false },
		},
	}),
	summary({
		key: 'dns_cname',
		name: 'DNS CNAME',
		status: 'untested',
		capabilities: {
			keyword_search: false,
			filters: { remote: false, countries: false, contract: false },
		},
	}),
]

const PROFILE = {
	id: '00000000-0000-0000-0000-000000000001',
	name: 'Freelance Rust',
	active: true,
	revision: 3,
	created_at: '2026-10-01T10:00:00Z',
	updated_at: '2026-10-07T10:00:00Z',
	definition: {},
}

const UPGRADED_ATS_AGENT: JsonObject = {
	type: 'search',
	id: 'offres-missions',
	name: 'Offres de missions',
	enabled: true,
	providers: ['ats'],
	keywords: ['freelance Rust', 'mission Rust'],
	exclusions: [],
	filters: { remote: 'any', countries: [], contracts: [] },
	accept_min_keywords: 2,
}

function blank(): JsonObject {
	return toJsonObject(
		defaultValue(PROFILE_SCHEMA, PROFILE_SCHEMA, PROFILE_HINTS),
	)
}

let latest: JsonObject = blank()
let opened: string[] = []

beforeEach(() => {
	opened = []
})

function Layout({ agents }: { agents: JsonObject[] }) {
	const [seed] = useState<JsonObject>(() => ({ ...blank(), agents }))
	const editor = useDefinitionEditor(seed)
	const [tab, setTab] = useState<ProfileTab>('sourcing')
	const [agent, setAgent] = useState<string | null>(null)
	latest = editor.value
	return (
		<ProfileDetail
			state="ready"
			errorMessage={null}
			profile={PROFILE}
			activation={<button type="button">Désactiver</button>}
			tab={tab}
			tabs={[
				...editorTabItems([]),
				{ id: 'history', label: TAB_LABELS.history, problems: 0 },
				{ id: 'trials', label: TAB_LABELS.trials, problems: 0 },
			]}
			exporting={false}
			exportError={null}
			definition={
				<ProfileEditor
					form={
						<DefinitionTabContent
							tab={asEditorTab(tab)}
							value={editor.value}
							problems={[]}
							onChange={editor.set}
							catalog={CATALOG}
							onOpenProvider={(key) => opened.push(key)}
							agentId={agent}
							onAgentChange={(id) => setAgent(id)}
						/>
					}
					problems={[]}
					notice={null}
					dirty={editor.dirty}
					saving={false}
					saveLabel="Enregistrer comme nouvelle révision"
					confirmingReset={editor.confirmingReset}
					onRequestReset={editor.requestReset}
					onCancelReset={editor.cancelReset}
					onConfirmReset={editor.reset}
					onSave={() => undefined}
				/>
			}
			history={<p>Historique</p>}
			trials={<p>Essais</p>}
			onTabChange={(next) => {
				setTab(next)
				setAgent(null)
			}}
			onExport={() => undefined}
			onBack={() => undefined}
		/>
	)
}

function tile(key: string): HTMLElement {
	return document.querySelector(`[data-provider-tile="${key}"]`) as HTMLElement
}

function openAgent(name: string) {
	fireEvent.click(screen.getByRole('button', { name: `Modifier ${name}` }))
}

function searchAgent(providers: string[]): JsonObject {
	return {
		...newAgent('search', [], providers),
		name: 'Veille IAM',
		keywords: ['sso'],
	}
}

describe('sourcing agent page', () => {
	it('opens from the card inside the profile layout with the tabs visible', () => {
		render(<Layout agents={[UPGRADED_ATS_AGENT]} />)
		openAgent('Offres de missions')
		expect(
			screen.getByRole('button', { name: 'Retour aux profils' }),
		).toBeTruthy()
		expect(
			screen.getByRole('heading', { level: 1, name: 'Freelance Rust' }),
		).toBeTruthy()
		expect(screen.getByText('Révision 3')).toBeTruthy()
		expect(screen.getByRole('button', { name: 'Désactiver' })).toBeTruthy()
		expect(
			screen.getByRole('button', { name: 'Exporter en JSON' }),
		).toBeTruthy()
		expect(
			screen
				.getByRole('tab', { name: TAB_LABELS.sourcing })
				.getAttribute('aria-selected'),
		).toBe('true')
		expect(
			screen.getByRole('button', { name: 'Agents de sourcing' }),
		).toBeTruthy()
		expect(screen.getByLabelText("Nom de l'agent")).toHaveProperty(
			'value',
			'Offres de missions',
		)
		expect(
			screen.queryByRole('button', { name: 'Ajouter un agent' }),
		).toBeNull()
	})

	it('keeps a leftover ats provider selected so it can be removed', () => {
		render(<Layout agents={[UPGRADED_ATS_AGENT]} />)
		const card = document.querySelector('[data-agent="0"]') as HTMLElement
		expect(
			card.querySelector('[data-provider-logo="ats"]')?.getAttribute('title'),
		).toBe("Offres d'emploi de l'entreprise · Prêt")
		expect(within(card).queryByText('Aucune source choisie.')).toBeNull()
		openAgent('Offres de missions')
		expect(tile('ats').getAttribute('data-state')).toBe('selected')
		fireEvent.click(
			within(tile('ats')).getByRole('button', {
				name: "Offres d'emploi de l'entreprise",
			}),
		)
		expect(document.querySelector('[data-provider-tile="ats"]')).toBeNull()
	})

	it('edits the criteria into the profile draft and shows the unsaved bar', () => {
		render(<Layout agents={[searchAgent(['boamp'])]} />)
		expect(screen.queryByTestId('save-bar')).toBeNull()
		openAgent('Veille IAM')
		const criteria = within(screen.getByRole('region', { name: 'Critères' }))
		const keywords = criteria.getByLabelText('Mots-clés')
		fireEvent.change(keywords, { target: { value: 'keycloak' } })
		fireEvent.keyDown(keywords, { key: 'Enter' })
		const exclusions = criteria.getByLabelText('Exclusions')
		fireEvent.change(exclusions, { target: { value: 'stage' } })
		fireEvent.keyDown(exclusions, { key: 'Enter' })
		const remote = criteria.getByRole('radiogroup', { name: 'Télétravail' })
		expect(
			within(remote)
				.getAllByRole('radio')
				.map((radio) => radio.closest('label')?.textContent),
		).toEqual(['Indifférent', 'Full remote', 'Remote ou hybride'])
		fireEvent.click(within(remote).getByRole('radio', { name: 'Full remote' }))
		const contract = criteria.getByRole('group', { name: 'Contrat' })
		expect(
			within(contract)
				.getByRole('button', { name: 'Tous' })
				.getAttribute('aria-pressed'),
		).toBe('true')
		fireEvent.click(within(contract).getByRole('button', { name: 'Freelance' }))
		fireEvent.click(within(contract).getByRole('button', { name: 'Salarié' }))
		const code = COUNTRY_CODES[0] as string
		const country = criteria.getByRole('combobox', { name: 'Pays' })
		fireEvent.change(country, {
			target: { value: countryName(code) },
		})
		fireEvent.click(screen.getByRole('option', { name: countryName(code) }))
		expect(latest.agents).toMatchObject([
			{
				keywords: ['sso', 'keycloak'],
				exclusions: ['stage'],
				filters: {
					remote: 'full_remote',
					contracts: ['freelance', 'employee'],
					countries: [code],
				},
			},
		])
		fireEvent.click(within(contract).getByRole('button', { name: 'Tous' }))
		expect(latest.agents).toMatchObject([{ filters: { contracts: [] } }])
		const bar = within(screen.getByTestId('save-bar'))
		expect(
			bar.getByRole('button', { name: 'Enregistrer comme nouvelle révision' }),
		).toBeTruthy()
		expect(screen.getByLabelText("Nom de l'agent")).toBeTruthy()
	})

	it('adds and removes providers by clicking their tiles', () => {
		render(<Layout agents={[searchAgent(['boamp'])]} />)
		openAgent('Veille IAM')
		expect(tile('boamp').getAttribute('data-state')).toBe('selected')
		expect(tile('ted').getAttribute('data-state')).toBe('available')
		const selected = tile('boamp').className.split(' ')
		expect(selected).toEqual(
			expect.arrayContaining([
				'ring-2',
				'ring-emerald-500',
				'ring-offset-0',
				'bg-emerald-50',
				'border-transparent',
			]),
		)
		expect(tile('boamp').className).not.toMatch(
			/ring-primary|ring-foreground|border-2|ring-offset-1/,
		)
		const available = tile('ted').className
		expect(available).not.toMatch(/ring-|bg-emerald/)
		expect(available).toContain('border-emerald-500/70')
		const focus = within(tile('ted')).getByRole('button', { name: 'TED' })
		expect(focus.className).not.toMatch(/focus-visible:ring/)
		expect(focus.className).toContain('focus-visible:outline-dashed')
		expect(tile('ted').parentElement?.className).toContain('auto-fill')
		fireEvent.click(within(tile('ted')).getByRole('button', { name: 'TED' }))
		expect(latest.agents).toMatchObject([{ providers: ['boamp', 'ted'] }])
		expect(within(tile('ted')).getByText('Ajouté')).toBeTruthy()
		fireEvent.click(
			within(tile('boamp')).getByRole('button', { name: 'BOAMP' }),
		)
		expect(latest.agents).toMatchObject([{ providers: ['ted'] }])
		expect(tile('dns_cname')).toBeNull()
	})

	it('shows a provider that is not ready as dashed with a link to configure it', () => {
		render(<Layout agents={[searchAgent(['boamp'])]} />)
		openAgent('Veille IAM')
		const travail = tile('france_travail')
		expect(travail.getAttribute('data-state')).toBe('unready')
		expect(travail.className).toContain('border-dashed')
		expect(within(travail).queryByRole('button')).toBeNull()
		const link = within(travail).getByRole('link', {
			name: 'Configurer France Travail',
		})
		expect(link.textContent).toBe('À configurer')
		expect(link.getAttribute('href')).toBe('/sources/providers/france_travail')
		fireEvent.click(link)
		expect(opened).toEqual(['france_travail'])
		expect(latest.agents).toMatchObject([{ providers: ['boamp'] }])
	})

	it('lets a selected provider that is not ready go', () => {
		render(<Layout agents={[searchAgent(['boamp', 'france_travail'])]} />)
		openAgent('Veille IAM')
		expect(
			screen.getByText(
				"« France Travail » n'est pas prêt : l'agent l'ignorera tant qu'il n'est pas configuré et testé.",
			),
		).toBeTruthy()
		fireEvent.click(
			within(tile('france_travail')).getByRole('button', {
				name: 'France Travail',
			}),
		)
		expect(latest.agents).toMatchObject([{ providers: ['boamp'] }])
	})

	it('shows per provider what is native and what is applied after collection', () => {
		render(<Layout agents={[searchAgent(['boamp', 'ted'])]} />)
		openAgent('Veille IAM')
		const matrix = screen.getByRole('region', {
			name: 'Ce que chaque fournisseur applique',
		})
		expect(
			within(matrix)
				.getAllByRole('columnheader')
				.map((cell) => cell.textContent),
		).toEqual(['Critère', 'BOAMP', 'TED'])
		const cell = (id: string) =>
			matrix.querySelector(`[data-cell="${id}"]`)?.textContent
		expect(cell('countries:ted')).toBe('Natif')
		expect(cell('countries:boamp')).toBe('Après collecte')
		expect(cell('keywords:boamp')).toBe('Natif')
		expect(cell('exclusions:ted')).toBe('Après collecte')
	})

	it('explains the keyword threshold accurately under Avancé', () => {
		render(<Layout agents={[searchAgent(['boamp'])]} />)
		openAgent('Veille IAM')
		const advanced = screen
			.getByText('Avancé')
			.closest('details') as HTMLElement
		expect(advanced.hasAttribute('open')).toBe(false)
		expect(
			within(advanced).getByRole('radiogroup', {
				name: 'Mots-clés distincts pour un signal fort',
			}),
		).toBeTruthy()
		expect(within(advanced).getByLabelText('Identifiant')).toBeTruthy()
		expect(
			within(advanced).getByText(/gardée avec un signal plus faible/),
		).toBeTruthy()
		expect(
			screen.getByText(
				/Ils ne l'écartent pas : seule une annonce sans aucun mot-clé est écartée/,
			),
		).toBeTruthy()
		expect(document.body.textContent).not.toMatch(/LLM/)
	})

	it('keeps the page open while the name drives the id', () => {
		render(
			<Layout agents={[{ ...searchAgent(['boamp']), id: 'veille-iam' }]} />,
		)
		openAgent('Veille IAM')
		fireEvent.change(screen.getByLabelText("Nom de l'agent"), {
			target: { value: 'Missions Rust' },
		})
		expect(screen.getByLabelText("Nom de l'agent")).toHaveProperty(
			'value',
			'Missions Rust',
		)
		expect(latest.agents).toMatchObject([
			{ id: 'missions-rust', name: 'Missions Rust' },
		])
		fireEvent.change(screen.getByLabelText('Identifiant'), {
			target: { value: '' },
		})
		expect(screen.getByLabelText('Identifiant')).toHaveProperty('value', '')
		fireEvent.change(screen.getByLabelText('Identifiant'), {
			target: { value: 'rust' },
		})
		expect(screen.getByLabelText("Nom de l'agent")).toBeTruthy()
		expect(latest.agents).toMatchObject([{ id: 'rust' }])
	})

	it('goes back to the agent list and highlights the edited card', () => {
		const first = searchAgent(['boamp'])
		const second = { ...newAgent('search', [first], ['ted']), name: 'Autre' }
		render(<Layout agents={[first, second]} />)
		openAgent('Autre')
		fireEvent.click(screen.getByRole('button', { name: 'Agents de sourcing' }))
		expect(
			screen.getByRole('button', { name: 'Ajouter un agent' }),
		).toBeTruthy()
		const cards = document.querySelectorAll<HTMLElement>('[data-agent]')
		expect(cards[1]?.className).toContain('ring-2')
		expect(cards[0]?.className).not.toContain('ring-2')
	})

	it('leaves the agent page when another tab is chosen', () => {
		render(<Layout agents={[searchAgent(['boamp'])]} />)
		openAgent('Veille IAM')
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.qualification }))
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.sourcing }))
		expect(
			screen.getByRole('button', { name: 'Ajouter un agent' }),
		).toBeTruthy()
		expect(screen.queryByLabelText("Nom de l'agent")).toBeNull()
	})

	it('removes the agent after a confirmation and returns to the list', () => {
		render(<Layout agents={[searchAgent(['boamp'])]} />)
		openAgent('Veille IAM')
		fireEvent.click(screen.getByRole('button', { name: 'Supprimer' }))
		fireEvent.click(screen.getByRole('button', { name: 'Conserver' }))
		expect(screen.getByLabelText("Nom de l'agent")).toBeTruthy()
		fireEvent.click(screen.getByRole('button', { name: 'Supprimer' }))
		fireEvent.click(
			screen.getByRole('button', { name: 'Confirmer la suppression' }),
		)
		expect(latest.agents).toEqual([])
		expect(screen.getByText(/Aucun agent pour le moment/)).toBeTruthy()
	})

	it('toggles the agent from its page', () => {
		render(<Layout agents={[searchAgent(['boamp'])]} />)
		openAgent('Veille IAM')
		fireEvent.click(screen.getByRole('switch', { name: 'Activé' }))
		expect(latest.agents).toMatchObject([{ enabled: false }])
	})

	it('puts the settings of a signal agent left and its provider right', () => {
		render(<Layout agents={[{ ...newAgent('dns', []), name: 'Mon DNS' }]} />)
		openAgent('Mon DNS')
		const settings = within(screen.getByRole('region', { name: 'Réglages' }))
		expect(settings.getAllByText('Motifs CNAME').length).toBeGreaterThan(0)
		const provider = within(screen.getByRole('region', { name: 'Fournisseur' }))
		expect(provider.getByText('DNS CNAME')).toBeTruthy()
		expect(tile('dns_cname').getAttribute('data-status')).toBe('untested')
		expect(
			provider.getByRole('link', { name: 'Configurer DNS CNAME' }),
		).toBeTruthy()
		expect(
			screen.queryByRole('region', {
				name: 'Ce que chaque fournisseur applique',
			}),
		).toBeNull()
	})
})

function wizard() {
	return within(screen.getByRole('dialog'))
}

function currentStep(): string {
	return (
		document.querySelector('[aria-current="step"]')?.textContent?.trim() ?? ''
	)
}

describe('new agent wizard', () => {
	it('walks the three steps with validation, back and next, then creates the agent', () => {
		render(<Layout agents={[]} />)
		fireEvent.click(screen.getByRole('button', { name: 'Ajouter un agent' }))
		expect(wizard().getByText('Nouvel agent de sourcing')).toBeTruthy()
		expect(currentStep()).toContain('Type et fournisseurs')
		expect(wizard().getByRole('radio', { name: /^Recherche/ })).toHaveProperty(
			'checked',
			true,
		)
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		expect(wizard().getByRole('alert').textContent).toBe(
			'Choisir au moins un fournisseur.',
		)
		expect(currentStep()).toContain('Type et fournisseurs')
		expect(within(tile('france_travail')).queryByRole('button')).toBeNull()
		fireEvent.click(
			within(tile('boamp')).getByRole('button', { name: 'BOAMP' }),
		)
		expect(wizard().queryByRole('alert')).toBeNull()
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		expect(currentStep()).toContain('Critères')
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		expect(wizard().getByRole('alert').textContent).toBe(
			'Ajouter au moins un mot-clé.',
		)
		const keywords = wizard().getByLabelText('Mots-clés')
		fireEvent.change(keywords, { target: { value: 'keycloak' } })
		fireEvent.keyDown(keywords, { key: 'Enter' })
		fireEvent.click(wizard().getByRole('radio', { name: 'Remote ou hybride' }))
		fireEvent.click(wizard().getByRole('button', { name: 'Précédent' }))
		expect(currentStep()).toContain('Type et fournisseurs')
		expect(tile('boamp').getAttribute('data-state')).toBe('selected')
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		expect(wizard().getByLabelText('Retirer keycloak')).toBeTruthy()
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		expect(currentStep()).toContain('Récapitulatif')
		const recap = within(wizard().getByLabelText('Récapitulatif'))
		expect(recap.getByText('BOAMP')).toBeTruthy()
		expect(recap.getByText('keycloak')).toBeTruthy()
		expect(recap.getByText('Remote ou hybride')).toBeTruthy()
		expect(
			wizard().getByRole('region', {
				name: 'Ce que chaque fournisseur applique',
			}),
		).toBeTruthy()
		expect(latest.agents).toEqual([])
		fireEvent.change(wizard().getByLabelText('Nom'), {
			target: { value: 'Veille IAM' },
		})
		fireEvent.click(wizard().getByRole('button', { name: 'Créer' }))
		expect(screen.queryByRole('dialog')).toBeNull()
		expect(latest.agents).toMatchObject([
			{
				type: 'search',
				id: 'veille-iam',
				name: 'Veille IAM',
				providers: ['boamp'],
				keywords: ['keycloak'],
				filters: { remote: 'remote_or_hybrid' },
			},
		])
		const card = document.querySelector('[data-agent="0"]') as HTMLElement
		expect(card.getAttribute('data-highlighted')).toBe('true')
		expect(card.className).toContain('ring-2')
		expect(screen.getByTestId('save-bar')).toBeTruthy()
	})

	it('refuses to create an agent without a name', () => {
		render(<Layout agents={[]} />)
		fireEvent.click(screen.getByRole('button', { name: 'Ajouter un agent' }))
		fireEvent.click(within(tile('ted')).getByRole('button', { name: 'TED' }))
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		const keywords = wizard().getByLabelText('Mots-clés')
		fireEvent.change(keywords, { target: { value: 'sso' } })
		fireEvent.keyDown(keywords, { key: 'Enter' })
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		fireEvent.change(wizard().getByLabelText('Nom'), { target: { value: '' } })
		fireEvent.click(wizard().getByRole('button', { name: 'Créer' }))
		expect(wizard().getByRole('alert').textContent).toBe(
			"Donner un nom à l'agent.",
		)
		expect(latest.agents).toEqual([])
	})

	it('offers the three agent types with a one-line explanation', () => {
		render(<Layout agents={[]} />)
		fireEvent.click(screen.getByRole('button', { name: 'Ajouter un agent' }))
		const radios = wizard().getAllByRole('radio')
		expect(radios.map((radio) => radio.getAttribute('value'))).toEqual([
			'search',
			'certificates',
			'dns',
		])
		const group = wizard().getByRole('radiogroup', { name: "Type d'agent" })
		for (const kind of ['search', 'certificates', 'dns'] as const) {
			expect(within(group).getByText(KIND_META[kind].label)).toBeTruthy()
			expect(within(group).getByText(KIND_META[kind].description)).toBeTruthy()
		}
		expect(within(group).queryByText('Signaux techniques')).toBeNull()
	})

	it('creates a dns agent with its patterns editor', () => {
		render(<Layout agents={[]} />)
		fireEvent.click(screen.getByRole('button', { name: 'Ajouter un agent' }))
		fireEvent.click(wizard().getByRole('radio', { name: /^DNS/ }))
		expect(document.querySelector('[data-provider-tile]')).toBeNull()
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		expect(currentStep()).toContain('Critères')
		expect(wizard().getAllByText('Motifs CNAME').length).toBeGreaterThan(0)
		expect(wizard().queryByLabelText('Mots-clés')).toBeNull()
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		expect(wizard().getByRole('alert').textContent).toBe(
			'Ajouter au moins un motif CNAME.',
		)
		fireEvent.click(wizard().getByText('Ajouter un motif'))
		const row = document.querySelector<HTMLElement>(
			'[data-row="agents[0].patterns[0]"]',
		) as HTMLElement
		fireEvent.change(within(row).getByLabelText('Fournisseur'), {
			target: { value: 'Okta' },
		})
		fireEvent.change(within(row).getByLabelText('Suffixe cible'), {
			target: { value: 'okta.com' },
		})
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		const recap = within(wizard().getByLabelText('Récapitulatif'))
		expect(recap.getByText('DNS')).toBeTruthy()
		expect(recap.getByText('1 motif CNAME')).toBeTruthy()
		fireEvent.click(wizard().getByRole('button', { name: 'Créer' }))
		expect(latest.agents).toMatchObject([
			{
				type: 'dns',
				id: 'dns',
				patterns: [{ provider: 'Okta', target_suffix: 'okta.com' }],
			},
		])
	})

	it('creates a certificates agent with its labels tag input', () => {
		render(<Layout agents={[]} />)
		fireEvent.click(screen.getByRole('button', { name: 'Ajouter un agent' }))
		fireEvent.click(wizard().getByRole('radio', { name: /^Certificats/ }))
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		expect(
			wizard().getByText(/Libellés de nom d'hôte à rechercher/),
		).toBeTruthy()
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		expect(wizard().getByRole('alert').textContent).toBe(
			'Ajouter au moins un libellé.',
		)
		const labels = wizard().getByLabelText('Libellés')
		fireEvent.change(labels, { target: { value: 'sso' } })
		fireEvent.keyDown(labels, { key: 'Enter' })
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		fireEvent.click(wizard().getByRole('button', { name: 'Créer' }))
		expect(latest.agents).toMatchObject([
			{ type: 'certificates', labels: ['sso'] },
		])
	})

	it('edits the labels of a certificates agent on its page', () => {
		render(
			<Layout
				agents={[
					{
						...newAgent('certificates', []),
						name: 'Certificats IAM',
						labels: ['auth'],
					},
				]}
			/>,
		)
		openAgent('Certificats IAM')
		const settings = within(screen.getByRole('region', { name: 'Réglages' }))
		expect(settings.getByLabelText('Retirer auth')).toBeTruthy()
		const labels = settings.getByLabelText('Libellés')
		fireEvent.change(labels, { target: { value: 'login' } })
		fireEvent.keyDown(labels, { key: 'Enter' })
		expect(latest.agents).toMatchObject([{ labels: ['auth', 'login'] }])
	})

	it('picks the target countries of a certificates agent on its page', () => {
		render(
			<Layout
				agents={[
					{
						...newAgent('certificates', []),
						name: 'Certificats IAM',
						labels: ['auth'],
					},
				]}
			/>,
		)
		openAgent('Certificats IAM')
		const settings = within(screen.getByRole('region', { name: 'Réglages' }))
		expect(settings.getByText(/hors pays visé/)).toBeTruthy()
		const country = settings.getByRole('combobox', { name: 'Pays' })
		fireEvent.change(country, { target: { value: countryName('FR') } })
		fireEvent.click(screen.getByRole('option', { name: countryName('FR') }))
		expect(latest.agents).toMatchObject([
			{ labels: ['auth'], countries: ['FR'] },
		])
		fireEvent.click(
			settings.getByRole('button', { name: `Retirer ${countryName('FR')}` }),
		)
		expect(latest.agents).toMatchObject([{ countries: [] }])
	})

	it('keeps the countries of a dns agent at the agent level', () => {
		render(
			<Layout
				agents={[
					{
						...newAgent('dns', []),
						name: 'DNS IAM',
						patterns: [{ provider: 'Okta', target_suffix: 'okta.com' }],
						countries: ['FR'],
					},
				]}
			/>,
		)
		openAgent('DNS IAM')
		const settings = within(screen.getByRole('region', { name: 'Réglages' }))
		expect(settings.getByText(countryName('FR'))).toBeTruthy()
		const country = settings.getByRole('combobox', { name: 'Pays' })
		fireEvent.change(country, { target: { value: countryName('BE') } })
		fireEvent.click(screen.getByRole('option', { name: countryName('BE') }))
		expect(latest.agents).toMatchObject([{ countries: ['FR', 'BE'] }])
		expect(JSON.stringify(latest.agents)).not.toContain('"filters"')
	})

	it('closes without touching the draft', () => {
		render(<Layout agents={[]} />)
		fireEvent.click(screen.getByRole('button', { name: 'Ajouter un agent' }))
		fireEvent.click(
			within(tile('boamp')).getByRole('button', { name: 'BOAMP' }),
		)
		fireEvent.click(wizard().getByRole('button', { name: 'Annuler' }))
		expect(screen.queryByRole('dialog')).toBeNull()
		expect(latest.agents).toEqual([])
		expect(screen.queryByTestId('save-bar')).toBeNull()
	})

	it('gives a second agent of the same type a distinct id', () => {
		render(<Layout agents={[searchAgent(['boamp'])]} />)
		fireEvent.click(screen.getByRole('button', { name: 'Ajouter un agent' }))
		fireEvent.click(within(tile('ted')).getByRole('button', { name: 'TED' }))
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		const keywords = wizard().getByLabelText('Mots-clés')
		fireEvent.change(keywords, { target: { value: 'sso' } })
		fireEvent.keyDown(keywords, { key: 'Enter' })
		fireEvent.click(wizard().getByRole('button', { name: 'Suivant' }))
		fireEvent.change(wizard().getByLabelText('Nom'), {
			target: { value: 'Veille IAM' },
		})
		fireEvent.click(wizard().getByRole('button', { name: 'Créer' }))
		expect(latest.agents).toMatchObject([
			{ id: 'recherche' },
			{ id: 'veille-iam' },
		])
	})
})
