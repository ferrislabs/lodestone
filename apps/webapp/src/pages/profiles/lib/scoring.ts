import {
	isJsonObject,
	type JsonObject,
	type JsonValue,
} from '#/lib/schema-form'
import {
	type AgentKind,
	agentKind,
	agentList,
} from '#/pages/profiles/lib/agents'

export const DISTINCT_SIGNALS = 'distinct_signals'

export interface SignalKindMeta {
	kind: string
	label: string
	description: string
	group: string
	takesValue: boolean
	emittedBy: readonly AgentKind[]
}

export const SIGNAL_KINDS: readonly SignalKindMeta[] = [
	{
		kind: 'product_detected',
		label: 'Product detected',
		description: 'A known product was fingerprinted on a host.',
		group: 'Infrastructure',
		takesValue: true,
		emittedBy: ['http_probe'],
	},
	{
		kind: 'debt_marker',
		label: 'Debt marker',
		description: 'A technical debt marker was found on a host.',
		group: 'Infrastructure',
		takesValue: true,
		emittedBy: ['http_probe'],
	},
	{
		kind: 'saas_provider',
		label: 'SaaS provider',
		description: 'A domain points to a known SaaS provider.',
		group: 'Infrastructure',
		takesValue: true,
		emittedBy: ['dns_cname'],
	},
	{
		kind: 'public_tender',
		label: 'Public tender',
		description: 'A matching public procurement notice was published.',
		group: 'Public procurement',
		takesValue: false,
		emittedBy: ['boamp', 'ted'],
	},
	{
		kind: 'awarded_contract',
		label: 'Awarded contract',
		description: 'A matching public contract was awarded.',
		group: 'Public procurement',
		takesValue: false,
		emittedBy: ['decp'],
	},
	{
		kind: 'job_offer',
		label: 'Job offer',
		description: 'A matching job offer is open.',
		group: 'Job offers',
		takesValue: false,
		emittedBy: ['france_travail', 'ats'],
	},
	{
		kind: 'role_contact_found',
		label: 'Role contact found',
		description: 'A contact with a matching role was found.',
		group: 'Contacts',
		takesValue: false,
		emittedBy: [],
	},
	{
		kind: DISTINCT_SIGNALS,
		label: 'Distinct signal kinds',
		description: 'Counts how many different signal kinds the lead has.',
		group: 'Combined',
		takesValue: false,
		emittedBy: [],
	},
]

export const SIGNAL_GROUPS: readonly string[] = [
	'Infrastructure',
	'Public procurement',
	'Job offers',
	'Contacts',
	'Combined',
]

export interface Factor {
	signal: string
	weight: number
	value: string
}

export function signalMeta(kind: string): SignalKindMeta | null {
	return SIGNAL_KINDS.find((meta) => meta.kind === kind) ?? null
}

export function signalLabel(kind: string): string {
	return signalMeta(kind)?.label ?? (kind === '' ? 'New factor' : kind)
}

export function factorList(definition: JsonObject): JsonValue[] {
	const scoring = definition.scoring
	if (!isJsonObject(scoring)) return []
	return Array.isArray(scoring.factors) ? scoring.factors : []
}

export function readFactor(item: JsonValue | undefined): Factor {
	if (!isJsonObject(item)) return { signal: '', weight: 0, value: '' }
	return {
		signal: typeof item.signal === 'string' ? item.signal : '',
		weight: typeof item.weight === 'number' ? item.weight : 0,
		value: typeof item.value === 'string' ? item.value : '',
	}
}

export function factorTitle(factor: Pick<Factor, 'signal' | 'value'>): string {
	const label = signalLabel(factor.signal)
	return factor.value === '' ? label : `${label} · ${factor.value}`
}

function sourceItems(agent: JsonValue, field: string): JsonObject[] {
	if (!isJsonObject(agent) || !isJsonObject(agent.source)) return []
	const items = agent.source[field]
	return Array.isArray(items) ? items.filter(isJsonObject) : []
}

function stringField(item: JsonObject, key: string): string[] {
	const found = item[key]
	return typeof found === 'string' && found !== '' ? [found] : []
}

export function valueSuggestions(
	definition: JsonObject,
	kind: string,
): string[] {
	const found = new Set<string>()
	for (const agent of agentList(definition)) {
		const agentOf = agentKind(agent)
		if (agentOf === 'http_probe') {
			for (const fingerprint of sourceItems(agent, 'fingerprints')) {
				if (kind === 'product_detected') {
					for (const product of stringField(fingerprint, 'product')) {
						found.add(product)
					}
				}
				if (kind === 'debt_marker') {
					const markers = fingerprint.debt_markers
					if (Array.isArray(markers)) {
						for (const marker of markers.filter(isJsonObject)) {
							for (const id of stringField(marker, 'id')) found.add(id)
						}
					}
				}
			}
		}
		if (agentOf === 'dns_cname' && kind === 'saas_provider') {
			for (const pattern of sourceItems(agent, 'patterns')) {
				for (const provider of stringField(pattern, 'provider')) {
					found.add(provider)
				}
			}
		}
	}
	return [...found].sort()
}

export function isAvailable(definition: JsonObject, kind: string): boolean {
	const meta = signalMeta(kind)
	if (meta === null) return false
	if (meta.emittedBy.length === 0) return true
	return agentList(definition).some((agent) => {
		const agentOf = agentKind(agent)
		return agentOf !== null && meta.emittedBy.includes(agentOf)
	})
}

export function nextSignal(
	definition: JsonObject,
	factors: readonly Factor[],
): string {
	const used = new Set(factors.map((factor) => factor.signal))
	const free = SIGNAL_KINDS.find(
		(meta) => !used.has(meta.kind) && isAvailable(definition, meta.kind),
	)
	return free?.kind ?? 'job_offer'
}

export function maxWeight(factors: readonly Factor[]): number {
	return factors.reduce((max, factor) => Math.max(max, factor.weight), 0)
}
