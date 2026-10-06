import type { HostFact } from './types'

const ENVIRONMENT_LABELS: Readonly<Record<string, string>> = {
	production: 'prod',
	development: 'dev',
	staging: 'staging',
	test: 'test',
	preproduction: 'préprod',
	demo: 'démo',
}

const ENVIRONMENT_ORDER: readonly string[] = [
	'production',
	'preproduction',
	'staging',
	'test',
	'development',
	'demo',
]

export function environmentLabel(environment: string): string {
	return ENVIRONMENT_LABELS[environment] ?? environment
}

function rank(environment: string): number {
	const index = ENVIRONMENT_ORDER.indexOf(environment)
	return index === -1 ? ENVIRONMENT_ORDER.length : index
}

export function hostsSummary(hosts: readonly HostFact[]): string {
	if (hosts.length === 0) return 'Aucun hôte identifié'
	const counts = new Map<string, number>()
	for (const { environment } of hosts) {
		counts.set(environment, (counts.get(environment) ?? 0) + 1)
	}
	const groups = [...counts.entries()].sort(
		([a], [b]) => rank(a) - rank(b) || a.localeCompare(b),
	)
	const noun = hosts.length === 1 ? 'hôte' : 'hôtes'
	const [first] = groups
	const detail =
		groups.length === 1 && first
			? environmentLabel(first[0])
			: groups
					.map(
						([environment, count]) =>
							`${count} ${environmentLabel(environment)}`,
					)
					.join(', ')
	return `${hosts.length} ${noun} · ${detail}`
}

export function countryName(code: string, fallback: string): string {
	try {
		return (
			new Intl.DisplayNames(['fr'], { type: 'region' }).of(code) ?? fallback
		)
	} catch {
		return fallback
	}
}
