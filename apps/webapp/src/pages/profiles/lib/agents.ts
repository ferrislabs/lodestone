import { pluralize, sourceLabel } from '#/lib/labels'
import {
	classify,
	defaultValue,
	isJsonObject,
	type JsonObject,
	type JsonValue,
	type Problem,
	resolve,
	toJsonObject,
} from '#/lib/schema-form'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'

export const AGENT_KINDS = [
	'certificate_transparency',
	'dns_cname',
	'http_probe',
	'boamp',
	'decp',
	'ted',
	'france_travail',
	'ats',
] as const

export type AgentKind = (typeof AGENT_KINDS)[number]

export interface KindMeta {
	label: string
	description: string
	triaged: boolean
}

export const KIND_META: Record<AgentKind, KindMeta> = {
	certificate_transparency: {
		label: sourceLabel('certificate_transparency'),
		description:
			"Trouve des entreprises à partir des noms d'hôte des journaux de certificats publics.",
		triaged: false,
	},
	dns_cname: {
		label: sourceLabel('dns_cname'),
		description:
			'Identifie des fournisseurs à partir de la cible CNAME de domaines connus.',
		triaged: false,
	},
	http_probe: {
		label: sourceLabel('http_probe'),
		description:
			"Sonde des hôtes en HTTP à la recherche d'empreintes de produits.",
		triaged: false,
	},
	boamp: {
		label: sourceLabel('boamp'),
		description:
			'Recherche par mots-clés des avis de marchés publics français.',
		triaged: true,
	},
	decp: {
		label: sourceLabel('decp'),
		description:
			'Recherche par mots-clés des données de marchés publics français.',
		triaged: true,
	},
	ted: {
		label: sourceLabel('ted'),
		description: "Recherche par mots-clés des appels d'offres européens.",
		triaged: true,
	},
	france_travail: {
		label: sourceLabel('france_travail'),
		description: "Recherche par mots-clés des offres d'emploi France Travail.",
		triaged: true,
	},
	ats: {
		label: sourceLabel('ats'),
		description:
			"Recherche les offres d'emploi des entreprises déjà détectées.",
		triaged: true,
	},
}

export const KIND_GROUPS: { title: string; kinds: readonly AgentKind[] }[] = [
	{
		title: 'Infrastructure',
		kinds: ['certificate_transparency', 'dns_cname', 'http_probe'],
	},
	{ title: 'Marchés publics', kinds: ['boamp', 'decp', 'ted'] },
	{ title: "Offres d'emploi", kinds: ['france_travail', 'ats'] },
]

export function isAgentKind(value: unknown): value is AgentKind {
	return AGENT_KINDS.some((kind) => kind === value)
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
	const source = agent.source
	if (!isJsonObject(source)) return null
	return isAgentKind(source.kind) ? source.kind : null
}

export function agentField(agent: JsonValue | undefined, key: string): string {
	if (!isJsonObject(agent)) return ''
	const field = agent[key]
	return typeof field === 'string' ? field : ''
}

export function agentEnabled(agent: JsonValue | undefined): boolean {
	return isJsonObject(agent) ? agent.enabled !== false : true
}

export function agentCriteria(agent: JsonValue | undefined): string {
	if (!isJsonObject(agent)) return ''
	const llm = agent.llm
	if (!isJsonObject(llm)) return ''
	const criteria = llm.relevance_criteria
	return typeof criteria === 'string' ? criteria : ''
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
	return kind.replace(/_/g, '-')
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

function sourceVariant(kind: AgentKind) {
	const agent = resolve(
		PROFILE_SCHEMA,
		PROFILE_SCHEMA.properties?.agents?.items ?? {},
	)
	const source = agent.properties?.source
	if (source === undefined) return null
	const node = classify(PROFILE_SCHEMA, source)
	if (node.kind !== 'union') return null
	return node.variants.find((variant) => variant.value === kind) ?? null
}

export function sourceSchema(kind: AgentKind) {
	return sourceVariant(kind)?.schema ?? {}
}

export function defaultSource(kind: AgentKind): JsonObject {
	const variant = sourceVariant(kind)
	if (variant === null) return { kind }
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
): JsonObject {
	return {
		id: uniqueId(kindSlug(kind), idsExcept(agents, null)),
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

function sourceCount(agent: JsonValue | undefined, field: string): number {
	if (!isJsonObject(agent) || !isJsonObject(agent.source)) return 0
	const items = agent.source[field]
	return Array.isArray(items) ? items.length : 0
}

export function agentSummary(agent: JsonValue | undefined): string {
	const kind = agentKind(agent)
	if (kind === null) return ''
	const parts: string[] = []
	if (kind === 'certificate_transparency') {
		const labels = sourceCount(agent, 'labels')
		parts.push(`${labels} ${pluralize(labels, 'label', 'labels')}`)
	} else if (kind === 'dns_cname') {
		const patterns = sourceCount(agent, 'patterns')
		parts.push(
			`${patterns} ${pluralize(patterns, 'motif CNAME', 'motifs CNAME')}`,
		)
	} else if (kind === 'http_probe') {
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
	} else {
		const keywords = sourceCount(agent, 'keywords')
		parts.push(`${keywords} ${pluralize(keywords, 'mot-clé', 'mots-clés')}`)
		const exclusions = sourceCount(agent, 'exclusions')
		if (exclusions > 0) {
			parts.push(
				`${exclusions} ${pluralize(exclusions, 'exclusion', 'exclusions')}`,
			)
		}
	}
	return parts.join(' · ')
}

export interface DependencyNote {
	text: string
	warn: boolean
}

export function dependencyNote(
	kind: AgentKind | null,
	agents: readonly JsonValue[],
): DependencyNote | null {
	const kinds = agents.map(agentKind)
	if (kind === 'http_probe') {
		return {
			text: 'Sonde les hôtes trouvés par un agent Transparence des certificats.',
			warn: !kinds.includes('certificate_transparency'),
		}
	}
	if (kind === 'ats') {
		return {
			text: 'Ne visite que les entreprises déjà détectées par des agents Transparence des certificats, Sonde HTTP ou DNS CNAME.',
			warn: !kinds.some(
				(other) =>
					other === 'certificate_transparency' ||
					other === 'http_probe' ||
					other === 'dns_cname',
			),
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
	})
	return problems
}
