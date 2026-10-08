import {
	isJsonObject,
	type JsonObject,
	type JsonValue,
} from '#/lib/schema-form'
import type { ProviderSummary } from '#/pages/source-providers/lib/types'

export const REMOTE_OPTIONS = [
	{ value: 'any', label: 'Indifférent' },
	{ value: 'full_remote', label: 'Full remote' },
	{ value: 'remote_or_hybrid', label: 'Remote ou hybride' },
] as const

export const CONTRACT_OPTIONS = [
	{ value: 'freelance', label: 'Freelance' },
	{ value: 'employee', label: 'Salarié' },
	{ value: 'public_contract', label: 'Marché public' },
	{ value: 'any', label: 'Tous' },
] as const

export const UNKNOWN_PROVIDER_STATUS = 'unknown'

const FILTER_LABELS = [
	{ key: 'remote', label: 'Télétravail' },
	{ key: 'countries', label: 'Pays' },
	{ key: 'contract', label: 'Contrat' },
] as const

export function searchProviders(
	catalog: readonly ProviderSummary[],
): ProviderSummary[] {
	return catalog.filter((provider) => provider.capabilities.keyword_search)
}

export function findProvider(
	catalog: readonly ProviderSummary[],
	key: string,
): ProviderSummary | undefined {
	return catalog.find((provider) => provider.key === key)
}

export function providerName(
	catalog: readonly ProviderSummary[],
	key: string,
): string {
	return findProvider(catalog, key)?.name ?? key
}

export function providerStatusOf(
	catalog: readonly ProviderSummary[],
	key: string,
): string {
	return findProvider(catalog, key)?.status ?? UNKNOWN_PROVIDER_STATUS
}

export function isReadyProvider(
	catalog: readonly ProviderSummary[],
	key: string,
): boolean {
	return providerStatusOf(catalog, key) === 'ready'
}

export function notReadyProviders(
	providers: readonly string[],
	catalog: readonly ProviderSummary[],
): string[] {
	return providers.filter((key) => !isReadyProvider(catalog, key))
}

export interface CapabilityHint {
	native: string[]
	after: string[]
}

export function capabilityHint(provider: ProviderSummary): CapabilityHint {
	const native: string[] = []
	const after: string[] = []
	for (const { key, label } of FILTER_LABELS) {
		;(provider.capabilities.filters[key] ? native : after).push(label)
	}
	return { native, after }
}

export function capabilityText(hint: CapabilityHint): string {
	const native = hint.native.length > 0 ? hint.native.join(', ') : 'aucun'
	const after = hint.after.length > 0 ? hint.after.join(', ') : 'aucun'
	return `Filtré nativement : ${native} ; appliqué après la collecte : ${after}`
}

export function agentRemote(agent: JsonValue | undefined): string {
	if (!isJsonObject(agent) || !isJsonObject(agent.filters)) return 'any'
	const remote = agent.filters.remote
	return typeof remote === 'string' ? remote : 'any'
}

export function agentContracts(agent: JsonValue | undefined): string[] {
	if (!isJsonObject(agent) || !isJsonObject(agent.filters)) return []
	const contracts = agent.filters.contracts
	return Array.isArray(contracts)
		? contracts.filter((item): item is string => typeof item === 'string')
		: []
}

export function withProvider(
	agent: JsonObject,
	key: string,
	attached: boolean,
): JsonObject {
	const current = Array.isArray(agent.providers)
		? agent.providers.filter((item): item is string => typeof item === 'string')
		: []
	const next = attached
		? current.includes(key)
			? current
			: [...current, key]
		: current.filter((item) => item !== key)
	return { ...agent, providers: next }
}

export function withFilter(
	agent: JsonObject,
	field: 'remote' | 'contracts' | 'countries',
	value: JsonValue,
): JsonObject {
	const filters = isJsonObject(agent.filters) ? agent.filters : {}
	return { ...agent, filters: { ...filters, [field]: value } }
}

export function allContracts(agent: JsonValue | undefined): boolean {
	const contracts = agentContracts(agent)
	return contracts.length === 0 || contracts.includes('any')
}

export function withContract(
	agent: JsonObject,
	contract: string,
	selected: boolean,
): JsonObject {
	if (contract === 'any') {
		return withFilter(agent, 'contracts', selected ? [] : agentContracts(agent))
	}
	const current = agentContracts(agent).filter((item) => item !== 'any')
	const next = selected
		? current.includes(contract)
			? current
			: [...current, contract]
		: current.filter((item) => item !== contract)
	return withFilter(agent, 'contracts', next)
}

export function agentCountries(agent: JsonValue | undefined): string[] {
	if (!isJsonObject(agent) || !isJsonObject(agent.filters)) return []
	const countries = agent.filters.countries
	return Array.isArray(countries)
		? countries.filter((item): item is string => typeof item === 'string')
		: []
}

export function withCountries(
	agent: JsonObject,
	countries: readonly string[],
): JsonObject {
	return withFilter(agent, 'countries', [...countries])
}

export function agentKeywordCount(agent: JsonValue | undefined): number {
	if (!isJsonObject(agent) || !Array.isArray(agent.keywords)) return 0
	return agent.keywords.filter(
		(item) => typeof item === 'string' && item.trim() !== '',
	).length
}
