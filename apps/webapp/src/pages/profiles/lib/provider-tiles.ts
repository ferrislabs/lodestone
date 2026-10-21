import {
	searchProviders,
	UNKNOWN_PROVIDER_STATUS,
} from '#/pages/profiles/lib/search-agent'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'

export type TileState = 'selected' | 'available' | 'unready'

export interface ProviderTile {
	key: string
	name: string
	status: string
	state: TileState
	ready: boolean
	selectable: boolean
}

function tile(
	key: string,
	name: string,
	status: string,
	selected: boolean,
): ProviderTile {
	const ready = status === 'ready'
	return {
		key,
		name,
		status,
		state: selected ? 'selected' : ready ? 'available' : 'unready',
		ready,
		selectable: selected || ready,
	}
}

export function providerTiles(
	catalog: readonly ProviderSummary[],
	attached: readonly string[],
): ProviderTile[] {
	const listed = searchProviders(catalog).map((provider) =>
		tile(
			provider.key,
			provider.name,
			provider.status,
			attached.includes(provider.key),
		),
	)
	const missing = attached
		.filter((key) => !listed.some((known) => known.key === key))
		.map((key) => {
			const provider = catalog.find((candidate) => candidate.key === key)
			return tile(
				key,
				provider?.name ?? key,
				provider?.status ?? UNKNOWN_PROVIDER_STATUS,
				true,
			)
		})
	return [...listed, ...missing]
}
