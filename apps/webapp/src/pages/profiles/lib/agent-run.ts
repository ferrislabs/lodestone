import { pluralize } from '#/lib/labels'
import { providerName } from '#/pages/profiles/lib/search-agent'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'
import { triggerResultLabel } from '#/pages/sources/lib/state'
import type { TriggerResult } from '#/pages/sources/lib/types'

export interface AgentRunState {
	pending: boolean
	message: string | null
	error: boolean
}

export const IDLE_RUN: AgentRunState = {
	pending: false,
	message: null,
	error: false,
}

export function runAgent(
	providers: readonly string[],
	trigger: (provider: string) => void,
) {
	for (const provider of new Set(providers)) trigger(provider)
}

function sources(count: number, state: string): string {
	return `${count} ${pluralize(count, 'source', 'sources')} ${pluralize(count, state, `${state}s`)}`
}

export function agentRunState(
	providers: readonly string[],
	pending: ReadonlySet<string>,
	results: Readonly<Record<string, TriggerResult>>,
	catalog: readonly ProviderSummary[],
): AgentRunState {
	const keys = [...new Set(providers)]
	if (keys.some((key) => pending.has(key))) {
		return { pending: true, message: null, error: false }
	}
	const settled = keys.flatMap((key) => {
		const result = results[key]
		return result ? [{ key, result }] : []
	})
	if (settled.length === 0) return IDLE_RUN
	const errors = settled.filter(({ result }) => result.kind === 'error')
	const error = errors.length > 0
	if (keys.length === 1 && settled[0]) {
		return {
			pending: false,
			message: triggerResultLabel(settled[0].result),
			error,
		}
	}
	const scheduled = settled.filter(
		({ result }) => result.kind === 'scheduled',
	).length
	const running = settled.filter(
		({ result }) => result.kind === 'running',
	).length
	if (scheduled === settled.length) {
		return {
			pending: false,
			message: `Agent lancé : ${sources(scheduled, 'planifiée')}`,
			error,
		}
	}
	const parts: string[] = []
	if (scheduled > 0) parts.push(sources(scheduled, 'planifiée'))
	if (running > 0) parts.push(`${running} déjà en cours`)
	for (const { key, result } of errors) {
		parts.push(`${providerName(catalog, key)} : ${triggerResultLabel(result)}`)
	}
	return { pending: false, message: parts.join(' · '), error }
}
