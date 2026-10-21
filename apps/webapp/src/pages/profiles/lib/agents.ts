import { pluralize } from '#/lib/labels'
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

export const SIGNAL_KINDS = ['certificates', 'dns'] as const

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
			'Cherche des mots-clés dans une ou plusieurs sources du catalogue, avec des critères communs.',
	},
	certificates: {
		label: 'Certificats',
		description:
			"Repère dans les certificats publics les noms d'hôte qui contiennent vos libellés.",
	},
	dns: {
		label: 'DNS',
		description:
			'Reconnaît un fournisseur SaaS à la cible CNAME des hôtes connus.',
	},
}

export const SIGNAL_PROVIDERS: Record<SignalKind, string> = {
	certificates: 'certificate_transparency',
	dns: 'dns_cname',
}

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
	return isSignalKind(agent.type) ? agent.type : null
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
	return kind === null || kind === 'search' ? [] : [SIGNAL_PROVIDERS[kind]]
}

export function agentTypeLabel(agent: JsonValue | undefined): string {
	const kind = agentKind(agent)
	return kind === null ? '' : KIND_META[kind].label
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

function agentVariant(type: AgentKind) {
	const node = classify(
		PROFILE_SCHEMA,
		PROFILE_SCHEMA.properties?.agents?.items ?? {},
	)
	if (node.kind !== 'union') return null
	return node.variants.find((variant) => variant.value === type) ?? null
}

const COMMON_FIELDS = ['type', 'id', 'name', 'enabled']
const OWN_EDITOR_FIELDS = ['countries']

export function settingsSchema(kind: SignalKind): Schema {
	const schema = agentVariant(kind)?.schema ?? {}
	const keys = Object.keys(schema.properties ?? {}).filter(
		(key) => !COMMON_FIELDS.includes(key) && !OWN_EDITOR_FIELDS.includes(key),
	)
	return pickProperties(
		kind,
		keys,
		(schema.required ?? []).filter((key) => keys.includes(key)),
	)
}

function pickProperties(
	kind: AgentKind,
	keys: readonly string[],
	required: readonly string[],
): Schema {
	const properties = agentVariant(kind)?.schema.properties ?? {}
	const picked: Record<string, Schema> = {}
	for (const key of keys) {
		const property = properties[key]
		if (property !== undefined) picked[key] = property
	}
	return { properties: picked, required: [...required] }
}

export function keywordFieldsSchema(): Schema {
	return pickProperties('search', ['keywords', 'exclusions'], ['keywords'])
}

export function advancedFieldsSchema(): Schema {
	return pickProperties('search', ['accept_min_keywords'], [])
}

function defaultSettings(kind: SignalKind): JsonObject {
	const variant = agentVariant(kind)
	if (variant === null) return {}
	return toJsonObject(
		defaultValue(PROFILE_SCHEMA, variant.schema, PROFILE_HINTS, ['agents', 0]),
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
		...defaultSettings(kind),
		type: kind,
		id,
		name: KIND_META[kind].label,
		enabled: true,
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
	} else if (kind === 'certificates') {
		const labels = listCount(agent, 'labels')
		parts.push(`${labels} ${pluralize(labels, 'label', 'labels')}`)
	} else {
		const patterns = listCount(agent, 'patterns')
		parts.push(
			`${patterns} ${pluralize(patterns, 'motif CNAME', 'motifs CNAME')}`,
		)
	}
	return parts.join(' · ')
}

export interface DependencyNote {
	text: string
	warn: boolean
}

export function dependencyNote(
	agent: JsonValue | undefined,
	agents: readonly JsonValue[],
): DependencyNote | null {
	if (agentKind(agent) !== 'dns') return null
	return {
		text: 'Résout les hôtes trouvés par un agent Certificats, importés ou dérivés des leads.',
		warn: !agents.map(agentKind).includes('certificates'),
	}
}

const SLUG = /^[a-z0-9][a-z0-9-]{0,62}$/

function enrichmentEnabled(value: JsonObject): boolean {
	const enrichment = value.enrichment
	if (!isJsonObject(enrichment)) return false
	return ['http_probe', 'company_jobs'].some((step) => {
		const settings = enrichment[step]
		return isJsonObject(settings) && settings.enabled === true
	})
}

export function validateAgents(value: JsonObject): Problem[] {
	const agents = agentList(value)
	if (agents.length === 0 && !enrichmentEnabled(value)) {
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
