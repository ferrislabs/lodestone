import {
	type EcosystemFinding,
	type FindingRow,
	findingRows,
} from '#/lib/ecosystem'
import { formatDay, formatNumber } from '#/lib/format'

type Details = Record<string, unknown>

const CATEGORY_ORDER: readonly string[] = [
	'cms',
	'framework',
	'hosting_cdn',
	'analytics',
	'chat',
	'crm_marketing',
	'ecommerce',
	'auth',
	'payment',
	'other',
]

const FUNCTION_LABELS: Readonly<Record<string, string>> = {
	engineering: 'Tech et ingénierie',
	sales: 'Commercial',
	marketing: 'Marketing',
	ops: 'Opérations et fonctions support',
	data: 'Data',
	product: 'Produit',
	support: 'Support client',
	other: 'Autres',
}

const DNS_ORDER: readonly string[] = [
	'mail_provider',
	'mail_gateway',
	'mail_server',
	'no_mx',
	'spf_service',
	'spf',
	'dmarc',
	'verified_tool',
	'saas_host',
	'cname',
]

const DNS_TITLES: Readonly<Record<string, string>> = {
	mail_provider: 'Messagerie',
	mail_gateway: 'Filtrage des e-mails',
	mail_server: 'Serveur de messagerie',
	no_mx: 'Messagerie',
	spf_service: "Envoi d'e-mails (SPF)",
	spf: 'SPF',
	dmarc: 'DMARC',
	verified_tool: 'Outils vérifiés (TXT)',
	saas_host: 'Services sur sous-domaines',
	cname: 'Alias DNS',
}

function text(details: Details, key: string): string | null {
	const value = details[key]
	return typeof value === 'string' && value.trim() !== '' ? value.trim() : null
}

function count(details: Details, key: string): number | null {
	const value = details[key]
	return typeof value === 'number' && Number.isFinite(value) ? value : null
}

function list(details: Details, key: string): Details[] {
	const value = details[key]
	return Array.isArray(value)
		? value.filter(
				(item): item is Details => typeof item === 'object' && item !== null,
			)
		: []
}

function detailsOf(finding: EcosystemFinding): Details {
	return finding.details ?? {}
}

export interface TechChip {
	key: string
	name: string
	version: string | null
	evidence: string | null
	sourceUrl: string
}

export interface TechGroup {
	category: string
	label: string
	chips: TechChip[]
}

export function techGroups(findings: readonly EcosystemFinding[]): TechGroup[] {
	const groups: TechGroup[] = []
	for (const finding of findings) {
		if (finding.kind !== 'technology') continue
		const details = detailsOf(finding)
		const category = text(details, 'category') ?? 'other'
		const chip: TechChip = {
			key: finding.key,
			name: text(details, 'name') ?? finding.value,
			version: text(details, 'version'),
			evidence: text(details, 'evidence'),
			sourceUrl: finding.source_url,
		}
		const known = groups.find((group) => group.category === category)
		if (known) {
			known.chips.push(chip)
		} else {
			groups.push({ category, label: finding.label, chips: [chip] })
		}
	}
	const rank = (category: string) => {
		const index = CATEGORY_ORDER.indexOf(category)
		return index === -1 ? CATEGORY_ORDER.length : index
	}
	return groups.sort(
		(left, right) => rank(left.category) - rank(right.category),
	)
}

export type TrendDirection = 'up' | 'down' | 'flat'

export interface HiringTrend {
	direction: TrendDirection
	label: string
}

export interface CountLine {
	label: string
	count: number
}

export interface HiringView {
	headline: string
	trend: HiringTrend | null
	functions: CountLine[]
	locations: CountLine[]
	contracts: CountLine[]
	remote: string | null
}

function trendOf(delta: number | null): HiringTrend | null {
	if (delta === null) return null
	if (delta === 0) {
		return { direction: 'flat', label: 'Stable depuis le dernier passage' }
	}
	const sign = delta > 0 ? '+' : '−'
	return {
		direction: delta > 0 ? 'up' : 'down',
		label: `${sign}${formatNumber(Math.abs(delta))} depuis le dernier passage`,
	}
}

function offersLabel(total: number): string {
	if (total === 0) return 'Aucune offre ouverte'
	return total === 1
		? '1 offre ouverte'
		: `${formatNumber(total)} offres ouvertes`
}

function countLines(
	items: readonly Details[],
	labelKey: string,
	labels: Readonly<Record<string, string>> = {},
): CountLine[] {
	return items.flatMap((item) => {
		const raw = text(item, labelKey)
		const total = count(item, 'count')
		if (raw === null || total === null) return []
		return [{ label: labels[raw] ?? raw, count: total }]
	})
}

export function hiringView(
	findings: readonly EcosystemFinding[],
): HiringView | null {
	const summary = findings.find((finding) => finding.kind === 'hiring_summary')
	if (!summary) return null
	const details = detailsOf(summary)
	const total = count(details, 'count') ?? 0
	const remote = count(details, 'remote') ?? 0
	return {
		headline: offersLabel(total),
		trend: trendOf(count(details, 'delta')),
		functions: countLines(
			list(details, 'functions'),
			'function',
			FUNCTION_LABELS,
		),
		locations: countLines(list(details, 'locations'), 'name'),
		contracts: countLines(list(details, 'contracts'), 'label'),
		remote:
			remote === 0
				? null
				: remote === 1
					? '1 offre mentionne le télétravail'
					: `${formatNumber(remote)} offres mentionnent le télétravail`,
	}
}

export interface RecentItemView {
	key: string
	title: string
	date: string | null
	url: string
}

export interface RecentView {
	freshness: string | null
	latest: string | null
	items: RecentItemView[]
}

export function recentView(
	findings: readonly EcosystemFinding[],
): RecentView | null {
	const items = findings
		.filter((finding) => finding.kind === 'recent_item')
		.map((finding) => {
			const date = text(detailsOf(finding), 'date')
			return {
				key: finding.key,
				title: finding.value,
				date: date === null ? null : formatDay(date),
				url: finding.source_url,
				sortable: date ?? '',
			}
		})
		.sort((left, right) => right.sortable.localeCompare(left.sortable))
		.map(({ sortable: _sortable, ...item }) => item)
	const freshness = findings.find((finding) => finding.kind === 'freshness')
	const latest = findings.find((finding) => finding.kind === 'latest_news')
	if (items.length === 0 && !freshness && !latest) return null
	const days = freshness ? count(detailsOf(freshness), 'days') : null
	const latestDay = freshness ? text(detailsOf(freshness), 'latest') : null
	return {
		freshness:
			days === null
				? null
				: `${days === 0 ? "Mis à jour aujourd'hui" : days === 1 ? 'Mis à jour il y a 1 jour' : `Mis à jour il y a ${formatNumber(days)} jours`}${latestDay === null ? '' : ` (${formatDay(latestDay)})`}`,
		latest: latest ? latest.value : null,
		items,
	}
}

export interface DnsLine {
	kind: string
	title: string
	values: string[]
}

export function dnsLines(findings: readonly EcosystemFinding[]): DnsLine[] {
	const lines: DnsLine[] = []
	for (const kind of DNS_ORDER) {
		const values = findings
			.filter((finding) => finding.kind === kind)
			.map((finding) => finding.value)
		if (values.length === 0) continue
		const title = DNS_TITLES[kind] ?? kind
		const known = lines.find((line) => line.title === title)
		if (known) {
			known.values.push(...values)
		} else {
			lines.push({ kind, title, values })
		}
	}
	return lines
}

export type ModuleView =
	| { kind: 'website'; groups: TechGroup[]; rows: FindingRow[] }
	| { kind: 'dns'; lines: DnsLine[]; rows: FindingRow[] }
	| { kind: 'hiring'; summary: HiringView | null; rows: FindingRow[] }
	| { kind: 'recent'; recent: RecentView | null; rows: FindingRow[] }
	| { kind: 'list'; rows: FindingRow[] }

const HIRING_HIDDEN = new Set(['hiring_summary'])
const RECENT_HIDDEN = new Set(['recent_item', 'freshness', 'latest_news'])

function rowsWithout(
	findings: readonly EcosystemFinding[],
	hidden: ReadonlySet<string>,
): FindingRow[] {
	return findingRows(findings.filter((finding) => !hidden.has(finding.kind)))
}

export function moduleView(
	module: string,
	findings: readonly EcosystemFinding[],
): ModuleView {
	switch (module) {
		case 'website_stack':
			return {
				kind: 'website',
				groups: techGroups(findings),
				rows: rowsWithout(findings, new Set(['technology'])),
			}
		case 'dns_email':
			return { kind: 'dns', lines: dnsLines(findings), rows: [] }
		case 'hiring_activity':
			return {
				kind: 'hiring',
				summary: hiringView(findings),
				rows: rowsWithout(findings, HIRING_HIDDEN),
			}
		case 'recent_activity':
			return {
				kind: 'recent',
				recent: recentView(findings),
				rows: rowsWithout(findings, RECENT_HIDDEN),
			}
		default:
			return { kind: 'list', rows: findingRows(findings) }
	}
}
