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
import { newAgent } from '#/pages/profiles/lib/agents'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
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
		key: 'certificate_transparency',
		name: 'Certificate Transparency',
		status: 'ready',
		capabilities: {
			keyword_search: false,
			filters: { remote: false, countries: false, contract: false },
		},
	}),
]

function blank(): JsonObject {
	return toJsonObject(
		defaultValue(PROFILE_SCHEMA, PROFILE_SCHEMA, PROFILE_HINTS),
	)
}

let latest: JsonObject = blank()
const opened: string[] = []

function Harness({ agents }: { agents: JsonObject[] }) {
	const [value, setValue] = useState<JsonObject>({ ...blank(), agents })
	latest = value
	return (
		<DefinitionTabContent
			tab="sourcing"
			value={value}
			problems={[]}
			onChange={setValue}
			catalog={CATALOG}
			onOpenProvider={(key) => opened.push(key)}
		/>
	)
}

function open(agent: JsonObject) {
	render(<Harness agents={[agent]} />)
	fireEvent.click(screen.getByRole('button', { name: /Modifier/ }))
	return within(screen.getByRole('dialog'))
}

function searchAgent(providers: string[]): JsonObject {
	return {
		...newAgent('search', [], providers),
		name: 'Veille IAM',
		keywords: ['sso'],
	}
}

describe('search agent card', () => {
	it('shows the type and the providers with their status', () => {
		render(<Harness agents={[searchAgent(['boamp', 'france_travail'])]} />)
		const card = within(document.querySelector('[data-agent]') as HTMLElement)
		expect(card.getByText('Recherche')).toBeTruthy()
		const chips = card.getByRole('list', { name: 'Fournisseurs' })
		const boamp = chips.querySelector('[data-provider="boamp"]')
		const travail = chips.querySelector('[data-provider="france_travail"]')
		expect(within(boamp as HTMLElement).getByText('Prêt')).toBeTruthy()
		expect(
			within(travail as HTMLElement).getByText('Non configuré'),
		).toBeTruthy()
	})

	it('labels a signal agent with its type and no provider chips', () => {
		render(
			<Harness
				agents={[{ ...newAgent('http_probe', []), name: 'Ma sonde' }]}
			/>,
		)
		const card = within(document.querySelector('[data-agent]') as HTMLElement)
		expect(card.getByText('Signal')).toBeTruthy()
		expect(card.getByText('Sonde HTTP')).toBeTruthy()
		expect(card.queryByRole('list', { name: 'Fournisseurs' })).toBeNull()
	})
})

describe('search agent sheet', () => {
	it('lists the keyword search providers of the catalog with their status', () => {
		const sheet = open(searchAgent(['boamp']))
		expect(sheet.getByLabelText('Nom')).toBeTruthy()
		const options = document.querySelectorAll('[data-provider-option]')
		expect(
			[...options].map((option) => option.getAttribute('data-provider-option')),
		).toEqual(['boamp', 'france_travail', 'ted'])
		expect(sheet.queryByLabelText('Certificate Transparency')).toBeNull()
		const boamp = sheet.getByLabelText('BOAMP') as HTMLInputElement
		expect(boamp.checked).toBe(true)
		expect(boamp.disabled).toBe(false)
	})

	it('disables a provider that is not ready and links to its catalog page', () => {
		const sheet = open(searchAgent(['boamp']))
		const travail = sheet.getByLabelText('France Travail') as HTMLInputElement
		expect(travail.disabled).toBe(true)
		const option = document.querySelector(
			'[data-provider-option="france_travail"]',
		) as HTMLElement
		expect(within(option).getByText('à configurer')).toBeTruthy()
		fireEvent.click(
			within(option).getByRole('button', { name: 'Configurer France Travail' }),
		)
		expect(opened).toEqual(['france_travail'])
	})

	it('attaches several providers to one agent', () => {
		const sheet = open(searchAgent(['boamp']))
		fireEvent.click(sheet.getByLabelText('TED'))
		expect(latest.agents).toMatchObject([{ providers: ['boamp', 'ted'] }])
		fireEvent.click(sheet.getByLabelText('BOAMP'))
		expect(latest.agents).toMatchObject([{ providers: ['ted'] }])
	})

	it('warns about an attached provider that is not ready and lets it go', () => {
		const sheet = open(searchAgent(['boamp', 'france_travail']))
		expect(
			sheet.getByText(
				"« France Travail » n'est pas prêt : l'agent l'ignorera tant qu'il n'est pas configuré et testé.",
			),
		).toBeTruthy()
		const travail = sheet.getByLabelText('France Travail') as HTMLInputElement
		expect(travail.disabled).toBe(false)
		fireEvent.click(travail)
		expect(latest.agents).toMatchObject([{ providers: ['boamp'] }])
	})

	it('edits the shared criteria', () => {
		const sheet = open(searchAgent(['boamp']))
		expect(sheet.getByLabelText('Mots-clés')).toBeTruthy()
		expect(sheet.getByLabelText('Exclusions')).toBeTruthy()
		fireEvent.click(sheet.getByLabelText('Remote ou hybride'))
		fireEvent.click(sheet.getByLabelText('Freelance'))
		fireEvent.click(sheet.getByLabelText('Salarié'))
		expect(latest.agents).toMatchObject([
			{
				filters: {
					remote: 'remote_or_hybrid',
					contracts: ['freelance', 'employee'],
				},
			},
		])
		fireEvent.click(sheet.getByLabelText('Freelance'))
		expect(latest.agents).toMatchObject([
			{ filters: { contracts: ['employee'] } },
		])
		const remote = sheet.getByRole('radiogroup', { name: 'Télétravail' })
		expect(
			within(remote)
				.getAllByRole('radio')
				.map((radio) => radio.parentElement?.textContent),
		).toEqual(['Indifférent', 'Full remote uniquement', 'Remote ou hybride'])
		expect(sheet.getByText('Pays')).toBeTruthy()
		expect(sheet.getByText('Tous les contrats')).toBeTruthy()
	})

	it('shows per provider what is filtered natively and what is applied after collection', () => {
		const sheet = open(searchAgent(['boamp', 'ted', 'france_travail']))
		const hints = document.querySelector(
			'[data-capability-hints]',
		) as HTMLElement
		expect(
			within(hints).getByText(
				'Filtré nativement : aucun ; appliqué après la collecte : Télétravail, Pays, Contrat',
				{ exact: false },
			),
		).toBeTruthy()
		expect(hints.querySelector('[data-hint="ted"]')?.textContent).toContain(
			'Filtré nativement : Pays ; appliqué après la collecte : Télétravail, Contrat',
		)
		expect(
			hints.querySelector('[data-hint="france_travail"]')?.textContent,
		).toContain(
			'Filtré nativement : Contrat ; appliqué après la collecte : Télétravail, Pays',
		)
		expect(sheet.getByText('Prise en charge par fournisseur')).toBeTruthy()
	})

	it('preselects the first ready provider on a new search agent', () => {
		render(<Harness agents={[]} />)
		fireEvent.click(screen.getByRole('button', { name: 'Ajouter un agent' }))
		fireEvent.click(
			within(screen.getByLabelText("Choisir un type d'agent")).getByText(
				'Recherche',
			),
		)
		expect(latest.agents).toMatchObject([
			{ type: 'search', providers: ['boamp'] },
		])
	})
})
