import { pluralize, sourceLabel } from '#/lib/labels'
import {
	classify,
	defaultValue,
	isJsonObject,
	type JsonObject,
	type JsonValue,
	type Problem,
	type Schema,
	toJsonObject,
} from '#/lib/schema-form'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'

export const SIGNAL_KINDS = [
	'certificate_transparency',
	'dns_cname',
	'http_probe',
] as const

export type SignalKind = (typeof SIGNAL_KINDS)[number]

export const AGENT_KINDS = ['search', ...SIGNAL_KINDS] as const

export type AgentKind = (typeof AGENT_KINDS)[number]

export interface KindMeta {
	label: string
	description: string
}

export const KIND_META: Record<AgentKind, KindMeta> = {
	search: {
		label: 'Recherche',
		description:
			'Recherche par mots-clés sur un ou plusieurs fournisseurs du catalogue, avec des critères communs.',
	},
	certificate_transparency: {
		label: sourceLabel('certificate_transparency'),
		description:
			"Trouve des entreprises à partir des noms d'hôte des journaux de certificats publics.",
	},
	dns_cname: {
		label: sourceLabel('dns_cname'),
		description:
			'Identifie des fournisseurs à partir de la cible CNAME de domaines connus.',
	},
	http_probe: {
		label: sourceLabel('http_probe'),
		description:
			"Sonde des hôtes en HTTP à la recherche d'empreintes de produits.",
	},
}

export const KIND_GROUPS: { title: string; kinds: readonly AgentKind[] }[] = [
	{ title: 'Fournisseurs du catalogue', kinds: ['search'] },
	{ title: 'Signaux techniques', kinds: SIGNAL_KINDS },
]

export function isAgentKind(value: unknown): value is AgentKind {
	return AGENT_KINDS.some((kind) => kind === value)
}

export function isSignalKind(value: unknown): value is SignalKind {
	return SIGNAL_KINDS.some((kind) => kind === value)
}

export function agentList(value: JsonObject): JsonValue[] {
	const agents = value.agents
	return Array.isArray(agents) ? agents : []
}

export function withAgents(
	value: JsonObject,
	agents: readonly JsonValue[],
): JsonObject {
	return { ...value, agents: [...agents] }
}

export function agentKind(agent: JsonValue | undefined): AgentKind | null {
	if (!isJsonObject(agent)) return null
	if (agent.type === 'search') return 'search'
	if (agent.type !== 'signal') return null
	const source = agent.source
	if (!isJsonObject(source)) return null
	return isSignalKind(source.provider) ? source.provider : null
}

export function agentProviders(agent: JsonValue | undefined): string[] {
	if (!isJsonObject(agent)) return []
	if (agent.type === 'search') {
		const providers = agent.providers
		return Array.isArray(providers)
			? providers.filter((key): key is string => typeof key === 'string')
			: []
	}
	const kind = agentKind(agent)
	return kind === null || kind === 'search' ? [] : [kind]
}

export function agentTypeLabel(agent: JsonValue | undefined): string {
	const kind = agentKind(agent)
	if (kind === null) return ''
	return kind === 'search' ? 'Recherche' : 'Signal'
}

export function agentField(agent: JsonValue | undefined, key: string): string {
	if (!isJsonObject(agent)) return ''
	const field = agent[key]
	return typeof field === 'string' ? field : ''
}

export function agentEnabled(agent: JsonValue | undefined): boolean {
	return isJsonObject(agent) ? agent.enabled !== false : true
}

export function slugify(text: string): string {
	return text
		.normalize('NFKD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '')
		.slice(0, 60)
		.replace(/-+$/g, '')
}

export function kindSlug(kind: AgentKind): string {
	return kind === 'search' ? 'recherche' : kind.replace(/_/g, '-')
}

export function uniqueId(base: string, taken: ReadonlySet<string>): string {
	if (!taken.has(base)) return base
	let suffix = 2
	while (taken.has(`${base}-${suffix}`)) suffix += 1
	return `${base}-${suffix}`
}

function idsExcept(
	agents: readonly JsonValue[],
	index: number | null,
): Set<string> {
	const ids = new Set<string>()
	agents.forEach((agent, position) => {
		if (position !== index) ids.add(agentField(agent, 'id'))
	})
	return ids
}

function agentVariant(type: 'search' | 'signal') {
	const node = classify(
		PROFILE_SCHEMA,
		PROFILE_SCHEMA.properties?.agents?.items ?? {},
	)
	if (node.kind !== 'union') return null
	return node.variants.find((variant) => variant.value === type) ?? null
}

function sourceVariant(kind: SignalKind) {
	const source = agentVariant('signal')?.schema.properties?.source
	if (source === undefined) return null
	const node = classify(PROFILE_SCHEMA, source)
	if (node.kind !== 'union') return null
	return node.variants.find((variant) => variant.value === kind) ?? null
}

export function sourceSchema(kind: SignalKind) {
	return sourceVariant(kind)?.schema ?? {}
}

function pickProperties(
	keys: readonly string[],
	required: readonly string[],
): Schema {
	const properties = agentVariant('search')?.schema.properties ?? {}
	const picked: Record<string, Schema> = {}
	for (const key of keys) {
		const property = properties[key]
		if (property !== undefined) picked[key] = property
	}
	return { properties: picked, required: [...required] }
}

export function keywordFieldsSchema(): Schema {
	return pickProperties(['keywords', 'exclusions'], ['keywords'])
}

export function advancedFieldsSchema(): Schema {
	return pickProperties(['accept_min_keywords'], [])
}

export function defaultSource(kind: SignalKind): JsonObject {
	const variant = sourceVariant(kind)
	if (variant === null) return { provider: kind }
	return toJsonObject(
		defaultValue(PROFILE_SCHEMA, variant.schema, PROFILE_HINTS, [
			'agents',
			0,
			'source',
		]),
	)
}

export function newAgent(
	kind: AgentKind,
	agents: readonly JsonValue[],
	providers: readonly string[] = [],
): JsonObject {
	const id = uniqueId(kindSlug(kind), idsExcept(agents, null))
	if (kind === 'search') {
		return {
			type: 'search',
			id,
			name: KIND_META.search.label,
			enabled: true,
			providers: [...providers],
			keywords: [],
			exclusions: [],
			filters: { remote: 'any', countries: [], contracts: [] },
			accept_min_keywords: 2,
		}
	}
	return {
		type: 'signal',
		id,
		name: KIND_META[kind].label,
		enabled: true,
		source: defaultSource(kind),
	}
}

function baseId(name: string, kind: AgentKind | null): string {
	return slugify(name) || (kind === null ? 'agent' : kindSlug(kind))
}

function isDerivedId(id: string, base: string): boolean {
	return id === base || id.replace(/-\d+$/, '') === base
}

export function renameAgent(
	agents: readonly JsonValue[],
	index: number,
	name: string,
): JsonValue[] {
	const agent = agents[index]
	if (!isJsonObject(agent)) return [...agents]
	const kind = agentKind(agent)
	const id = agentField(agent, 'id')
	const currentName = agentField(agent, 'name')
	const untouched =
		isDerivedId(id, baseId(currentName, kind)) ||
		(kind !== null &&
			currentName === KIND_META[kind].label &&
			isDerivedId(id, kindSlug(kind)))
	const next: JsonObject = { ...agent, name }
	if (untouched) {
		next.id = uniqueId(baseId(name, kind), idsExcept(agents, index))
	}
	return agents.map((item, position) => (position === index ? next : item))
}

function listCount(holder: JsonValue | undefined, field: string): number {
	if (!isJsonObject(holder)) return 0
	const items = holder[field]
	return Array.isArray(items) ? items.length : 0
}

function sourceCount(agent: JsonValue | undefined, field: string): number {
	return isJsonObject(agent) ? listCount(agent.source, field) : 0
}

export function agentSummary(agent: JsonValue | undefined): string {
	const kind = agentKind(agent)
	if (kind === null) return ''
	const parts: string[] = []
	if (kind === 'search') {
		const keywords = listCount(agent, 'keywords')
		parts.push(`${keywords} ${pluralize(keywords, 'mot-clé', 'mots-clés')}`)
		const exclusions = listCount(agent, 'exclusions')
		if (exclusions > 0) {
			parts.push(
				`${exclusions} ${pluralize(exclusions, 'exclusion', 'exclusions')}`,
			)
		}
	} else if (kind === 'certificate_transparency') {
		const labels = sourceCount(agent, 'labels')
		parts.push(`${labels} ${pluralize(labels, 'label', 'labels')}`)
	} else if (kind === 'dns_cname') {
		const patterns = sourceCount(agent, 'patterns')
		parts.push(
			`${patterns} ${pluralize(patterns, 'motif CNAME', 'motifs CNAME')}`,
		)
	} else {
		const fingerprints = sourceCount(agent, 'fingerprints')
		parts.push(
			`${fingerprints} ${pluralize(fingerprints, 'empreinte', 'empreintes')}`,
		)
		const excluded = sourceCount(agent, 'excluded_products')
		if (excluded > 0) {
			parts.push(
				`${excluded} ${pluralize(excluded, 'produit exclu', 'produits exclus')}`,
			)
		}
	}
	return parts.join(' · ')
}

export interface DependencyNote {
	text: string
	warn: boolean
}

const SIGNAL_FEEDERS: readonly (AgentKind | null)[] = [
	'certificate_transparency',
	'http_probe',
	'dns_cname',
]

export function dependencyNote(
	agent: JsonValue | undefined,
	agents: readonly JsonValue[],
): DependencyNote | null {
	const kind = agentKind(agent)
	const kinds = agents.map(agentKind)
	if (kind === 'http_probe') {
		return {
			text: 'Sonde les hôtes trouvés par un agent Transparence des certificats.',
			warn: !kinds.includes('certificate_transparency'),
		}
	}
	if (kind === 'search' && agentProviders(agent).includes('ats')) {
		return {
			text: 'Pages carrières : ne visite que les entreprises déjà détectées par des agents Transparence des certificats, Sonde HTTP ou DNS CNAME.',
			warn: !kinds.some((other) => SIGNAL_FEEDERS.includes(other)),
		}
	}
	return null
}

const SLUG = /^[a-z0-9][a-z0-9-]{0,62}$/

export function validateAgents(value: JsonObject): Problem[] {
	const agents = agentList(value)
	if (agents.length === 0) {
		return [{ path: 'agents', message: 'Doit contenir au moins un agent' }]
	}
	const problems: Problem[] = []
	const seen = new Set<string>()
	agents.forEach((agent, index) => {
		const id = agentField(agent, 'id')
		const path = `agents[${index}].id`
		if (id !== '' && !SLUG.test(id)) {
			problems.push({
				path,
				message: 'Doit être composé de minuscules, chiffres et tirets',
			})
		} else if (id !== '' && seen.has(id)) {
			problems.push({ path, message: 'Doit être unique dans le profil' })
		}
		seen.add(id)
		if (agentKind(agent) === 'search' && agentProviders(agent).length === 0) {
			problems.push({
				path: `agents[${index}].providers`,
				message: 'Choisir au moins un fournisseur',
			})
		}
	})
	return problems
}
