import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { crawlSettings } from '#/pages/source-providers/lib/fixtures'
import type { CrawlSettings } from '#/pages/source-providers/lib/types'
import { CrawlRulesFeature } from './crawl-rules-feature'

const server = vi.hoisted(() => ({
	settings: null as CrawlSettings | null,
	puts: [] as unknown[],
}))

function json(body: unknown): Response {
	return new Response(JSON.stringify(body), {
		status: 200,
		headers: { 'Content-Type': 'application/json' },
	})
}

beforeEach(() => {
	Object.assign(window, { apiUrl: 'http://api.test' })
	server.settings = crawlSettings()
	server.puts = []
	vi.stubGlobal(
		'fetch',
		vi.fn(async (url: URL | string, init?: RequestInit) => {
			expect(String(url)).toBe('http://api.test/api/v1/settings/crawl')
			if (init?.method?.toUpperCase() === 'PUT') {
				const body = JSON.parse(String(init.body)) as {
					robots_policy: string
				}
				server.puts.push(body)
				server.settings = crawlSettings({
					robots_policy: body.robots_policy,
					origin: 'organisation',
					updated_by_name: 'Alice Martin',
					updated_at: '2026-10-09T08:30:00Z',
				})
				return json({
					data: {
						settings: server.settings,
						dossiers_retried: body.robots_policy === 'ignore' ? 2 : 0,
					},
				})
			}
			return json({ data: server.settings })
		}),
	)
})

afterEach(() => {
	cleanup()
	vi.unstubAllGlobals()
})

function renderFeature() {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	})
	render(
		<QueryClientProvider client={client}>
			<CrawlRulesFeature open onOpenChange={vi.fn()} />
		</QueryClientProvider>,
	)
}

function robotsSwitch() {
	return screen.findByRole('switch', {
		name: 'Ignorer robots.txt pour la lecture des sites',
	})
}

describe('CrawlRulesFeature', () => {
	it('saves ignore only after the confirmation', async () => {
		renderFeature()
		fireEvent.click(await robotsSwitch())
		expect(await screen.findByRole('alertdialog')).toBeTruthy()
		expect(server.puts).toEqual([])

		fireEvent.click(screen.getByRole('button', { name: 'Ignorer robots.txt' }))

		await waitFor(() =>
			expect(server.puts).toEqual([{ robots_policy: 'ignore' }]),
		)
		expect(
			await screen.findByText(
				'2 dossiers bloqués par robots.txt seront relus.',
			),
		).toBeTruthy()
		expect(
			await screen.findByText(
				'Modifié par Alice Martin le 9 octobre 2026 à 10:30.',
			),
		).toBeTruthy()
		await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())
	})

	it('cancelling the confirmation saves nothing', async () => {
		renderFeature()
		fireEvent.click(await robotsSwitch())
		fireEvent.click(await screen.findByRole('button', { name: 'Annuler' }))
		await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())
		expect(server.puts).toEqual([])
		expect((await robotsSwitch()).getAttribute('aria-checked')).toBe('false')
	})

	it('respects robots.txt again without confirmation', async () => {
		server.settings = crawlSettings({ robots_policy: 'ignore' })
		renderFeature()
		const toggle = await robotsSwitch()
		expect(toggle.getAttribute('aria-checked')).toBe('true')
		fireEvent.click(toggle)
		await waitFor(() =>
			expect(server.puts).toEqual([{ robots_policy: 'respect' }]),
		)
		expect(screen.queryByRole('alertdialog')).toBeNull()
	})
})
