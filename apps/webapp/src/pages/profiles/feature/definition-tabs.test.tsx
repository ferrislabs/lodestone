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

class NoopResizeObserver {
	observe() {}
	unobserve() {}
	disconnect() {}
}

beforeAll(() => {
	vi.stubGlobal('ResizeObserver', NoopResizeObserver)
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
	const [value, setValue] = useState(initial)
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
				dirty
				saving={false}
				saveLabel="Create profile"
				blockedReason={
					agentList(value).length === 0 ? 'Add at least one agent.' : null
				}
				onSave={() => undefined}
			/>
		</>
	)
}

function addAgent(kind: AgentKind) {
	fireEvent.click(screen.getByRole('button', { name: 'Add agent' }))
	fireEvent.click(
		within(screen.getByLabelText('Choose a source')).getByText(
			KIND_META[kind].label,
		),
	)
}

describe('definition tabs', () => {
	it('shows only the subtree of the selected tab', () => {
		render(<Harness />)
		expect(screen.getByLabelText(/Offer/)).toBeTruthy()
		expect(screen.queryByLabelText(/Qualification threshold/)).toBeNull()
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.settings }))
		expect(screen.getByLabelText(/Qualification threshold/)).toBeTruthy()
		expect(screen.queryByLabelText(/Offer/)).toBeNull()
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.scoring }))
		expect(screen.getByRole('button', { name: 'Add factor' })).toBeTruthy()
		expect(screen.queryByLabelText(/Qualification threshold/)).toBeNull()
		fireEvent.click(screen.getByRole('tab', { name: TAB_LABELS.agents }))
		expect(screen.getByRole('button', { name: 'Add agent' })).toBeTruthy()
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
						message: 'must not be empty',
					},
				]}
			/>,
		)
		const tab = screen.getByRole('tab', { name: /Agents/ })
		expect(within(tab).getByLabelText('1 problems')).toBeTruthy()
		expect(
			within(screen.getByRole('tab', { name: /Identity/ })).queryByRole('img'),
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
						message: 'must not be empty',
					},
				]}
			/>,
		)
		expect(screen.queryByText('must not be empty')).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: /Expand/ }))
		expect(screen.getByText('must not be empty')).toBeTruthy()
	})

	it('shows the empty state and blocks creation without agents', () => {
		render(<Harness startTab="agents" />)
		expect(
			screen.getByText('No agent yet. Add one to start searching.'),
		).toBeTruthy()
		expect(
			screen
				.getByRole('button', { name: 'Create profile' })
				.hasAttribute('disabled'),
		).toBe(true)
		addAgent('boamp')
		expect(
			screen
				.getByRole('button', { name: 'Create profile' })
				.hasAttribute('disabled'),
		).toBe(false)
	})

	for (const kind of AGENT_KINDS) {
		it(`adds a ${kind} agent with only its fields`, async () => {
			render(<Harness startTab="agents" />)
			addAgent(kind)
			const card = document.querySelector<HTMLElement>('[data-agent="0"]')
			if (!card) throw new Error('card missing')
			const scoped = within(card)
			expect(scoped.getByLabelText('Name')).toHaveProperty(
				'value',
				KIND_META[kind].label,
			)
			await waitFor(() =>
				expect(document.activeElement).toBe(scoped.getByLabelText('Name')),
			)
			expect(scoped.queryAllByText('LLM criteria for this agent').length).toBe(
				KIND_META[kind].triaged ? 1 : 0,
			)
			const has = (text: string) => scoped.queryAllByText(text).length > 0
			if (kind === 'certificate_transparency') {
				expect(has('Labels')).toBe(true)
				expect(has('Keywords')).toBe(false)
			} else if (kind === 'dns_cname') {
				expect(has('Patterns')).toBe(true)
				expect(has('Labels')).toBe(false)
			} else if (kind === 'http_probe') {
				expect(has('Fingerprints')).toBe(true)
				expect(has('Excluded products')).toBe(true)
				expect(has('Keywords')).toBe(false)
			} else {
				expect(has('Keywords')).toBe(true)
				expect(has('Exclusions')).toBe(true)
				expect(has('Accept min keywords')).toBe(true)
				expect(has('Fingerprints')).toBe(false)
				expect(has('Labels')).toBe(false)
			}
		})
	}

	it('suffixes ids of agents of the same kind', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		addAgent('ats')
		expect(
			screen
				.getAllByLabelText('ID')
				.map((input) => (input as HTMLInputElement).value),
		).toEqual(['ats', 'ats-2'])
	})

	it('follows the name with the id until the id is edited', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		fireEvent.change(screen.getByLabelText('Name'), {
			target: { value: 'ATS English' },
		})
		expect(screen.getByLabelText('ID')).toHaveProperty('value', 'ats-english')
		fireEvent.change(screen.getByLabelText('ID'), { target: { value: 'mine' } })
		fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Z' } })
		expect(screen.getByLabelText('ID')).toHaveProperty('value', 'mine')
	})

	it('removes an agent after an inline confirmation', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		fireEvent.click(screen.getByRole('button', { name: /Remove agent/ }))
		fireEvent.click(screen.getByRole('button', { name: 'Keep' }))
		expect(screen.getAllByLabelText('ID')).toHaveLength(1)
		fireEvent.click(screen.getByRole('button', { name: /Remove agent/ }))
		fireEvent.click(screen.getByRole('button', { name: 'Confirm remove' }))
		expect(screen.queryAllByLabelText('ID')).toHaveLength(0)
		expect(screen.getByText(/No agent yet/)).toBeTruthy()
	})

	it('collapses and expands an agent', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		fireEvent.click(screen.getByRole('button', { name: /Collapse/ }))
		expect(screen.queryByLabelText('ID')).toBeNull()
		fireEvent.click(screen.getByRole('button', { name: /Expand/ }))
		expect(screen.getByLabelText('ID')).toBeTruthy()
	})

	it('warns when an http probe has no certificate transparency agent', () => {
		render(<Harness startTab="agents" />)
		addAgent('http_probe')
		expect(screen.getByRole('alert').textContent).toContain(
			'Probes hosts found by a Certificate transparency agent',
		)
		addAgent('certificate_transparency')
		expect(screen.queryByRole('alert')).toBeNull()
		expect(
			screen.getByText(/Probes hosts found by a Certificate transparency/),
		).toBeTruthy()
	})

	it('stores agent criteria and drops them when emptied', () => {
		render(<Harness startTab="agents" />)
		addAgent('ats')
		const field = screen.getByLabelText('LLM criteria for this agent')
		fireEvent.change(field, { target: { value: 'only devs' } })
		expect(field).toHaveProperty('value', 'only devs')
		fireEvent.change(field, { target: { value: '' } })
		expect(field).toHaveProperty('value', '')
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
