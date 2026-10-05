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
		description: 'Un domaine pointe vers un fournisseur SaaS connu.',
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
	weight: number | null
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
	if (!isJsonObject(item)) return { signal: '', weight: null, value: '' }
	return {
		signal: typeof item.signal === 'string' ? item.signal : '',
		weight: typeof item.weight === 'number' ? item.weight : null,
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

export const MIN_WEIGHT = 1
export const MAX_WEIGHT = 10
export const DEFAULT_WEIGHT = 5
export const WEIGHT_ERROR = 'must be an integer from 1 to 10'
export const THRESHOLD_ERROR = 'must be a percentage from 0 to 100'

export const WEIGHTS: readonly number[] = Array.from(
	{ length: MAX_WEIGHT },
	(_, index) => index + MIN_WEIGHT,
)

export function isValidWeight(weight: number | null): weight is number {
	return (
		weight !== null &&
		Number.isInteger(weight) &&
		weight >= MIN_WEIGHT &&
		weight <= MAX_WEIGHT
	)
}

export function isValidThreshold(
	threshold: number | null,
): threshold is number {
	return (
		threshold !== null &&
		Number.isInteger(threshold) &&
		threshold >= 0 &&
		threshold <= 100
	)
}

export function maxScore(
	factors: readonly Pick<Factor, 'signal' | 'weight'>[],
) {
	const kinds = new Set(
		factors
			.map((factor) => factor.signal)
			.filter((signal) => signal !== DISTINCT_SIGNALS),
	)
	return factors.reduce((total, factor) => {
		const weight = factor.weight ?? 0
		return (
			total +
			(factor.signal === DISTINCT_SIGNALS ? weight * kinds.size : weight)
		)
	}, 0)
}

export function qualificationPoints(thresholdPercent: number, max: number) {
	return (thresholdPercent * max) / 100
}

export function formatPoints(points: number): string {
	return String(Math.round(points * 10) / 10)
}

export function readThreshold(definition: JsonObject): number | null {
	const llm = definition.llm
	if (!isJsonObject(llm)) return null
	return typeof llm.qualification_threshold === 'number'
		? llm.qualification_threshold
		: null
}
