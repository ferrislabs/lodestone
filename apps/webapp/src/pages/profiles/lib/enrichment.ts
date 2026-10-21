import type { Schemas } from '#/api/api.client'
import {
	formatDuration,
	moduleStatusLabel,
	PROVIDER_NOT_READY,
	platformNotice,
	providerLink,
	type StatusTone,
	statusTone,
} from '#/lib/ecosystem'
import { type ModuleView, moduleView } from '#/lib/ecosystem-views'
import { formatDateTime, formatNumber } from '#/lib/format'
import {
	defaultValue,
	getIn,
	isJsonObject,
	type JsonObject,
	type Problem,
	resolve,
	type Schema,
	toJsonObject,
} from '#/lib/schema-form'
import { agentKind, agentList } from '#/pages/profiles/lib/agents'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import { belongsToPath } from '#/pages/profiles/lib/tabs'

export type EcosystemModuleInfo = Schemas.EcosystemModuleInfoDto
export type DryRunResult = Schemas.EcosystemDryRunDto

export const ENRICHMENT_WAIT_MINUTES = 30

function enrichmentSchema(): Schema {
	return resolve(PROFILE_SCHEMA, PROFILE_SCHEMA.properties?.enrichment ?? {})
}

function customSchema(): Schema {
	return resolve(PROFILE_SCHEMA, enrichmentSchema().properties?.custom ?? {})
}

function picked(schema: Schema, key: string): Schema {
	const property = schema.properties?.[key]
	return { properties: property === undefined ? {} : { [key]: property } }
}

export function inspectionEnabled(value: JsonObject): boolean {
	return getIn(value, ['inspection', 'enabled']) !== false
}

export function withInspectionEnabled(
	value: JsonObject,
	enabled: boolean,
): JsonObject {
	const section = isJsonObject(value.inspection) ? value.inspection : {}
	return { ...value, inspection: { ...section, enabled } }
}

export function customSection(value: JsonObject): JsonObject {
	const current = getIn(value, ['enrichment', 'custom'])
	return isJsonObject(current)
		? current
		: toJsonObject(
				defaultValue(PROFILE_SCHEMA, customSchema(), PROFILE_HINTS, [
					'enrichment',
					'custom',
				]),
			)
}

export function enrichmentSection(value: JsonObject): JsonObject {
	const current = value.enrichment
	return isJsonObject(current) ? current : { highlight_keywords: [] }
}

export function customFieldSchema(key: string): Schema {
	return picked(customSchema(), key)
}

export function enrichmentFieldSchema(key: string): Schema {
	return picked(enrichmentSchema(), key)
}

export function advancedHasProblem(problems: readonly Problem[]): boolean {
	return problems.some((problem) =>
		belongsToPath(problem.path, 'enrichment.custom'),
	)
}

export function searchKeywordUnion(value: JsonObject): string[] {
	const seen = new Set<string>()
	const union: string[] = []
	for (const agent of agentList(value)) {
		if (!isJsonObject(agent) || agentKind(agent) !== 'search') continue
		if (agent.enabled === false || !Array.isArray(agent.keywords)) continue
		for (const keyword of agent.keywords) {
			if (typeof keyword !== 'string') continue
			const key = keyword.trim().toLowerCase()
			if (key === '' || seen.has(key)) continue
			seen.add(key)
			union.push(keyword.trim())
		}
	}
	return union
}

export interface ModuleChip {
	label: string
	tone: StatusTone
}

export interface ModuleStat {
	label: string
	value: string
}

export interface ModuleExample {
	text: string
	domain: string
	sourceUrl: string
}

export interface ModuleCard {
	module: Schemas.EcosystemModule
	title: string
	label: string
	learns: string
	refresh: string
	chip: ModuleChip
	stats: ModuleStat[]
	example: ModuleExample | null
	canTest: boolean
	providerLink: string | null
}

function chipOf(module: EcosystemModuleInfo, enabled: boolean): ModuleChip {
	if (!module.available) return { label: 'À venir', tone: 'soon' }
	if (!enabled) return { label: 'Désactivé', tone: 'off' }
	if (!module.provider_ready)
		return { label: PROVIDER_NOT_READY, tone: 'failed' }
	return { label: 'Actif', tone: 'done' }
}

export function moduleCards(
	modules: readonly EcosystemModuleInfo[],
	enabled: boolean,
): ModuleCard[] {
	return modules.map((module) => {
		const stats = module.stats
		const example = stats.latest_example ?? null
		const lastRun = stats.last_run ?? null
		return {
			module: module.module,
			title: module.question,
			label: module.label,
			learns: module.learns,
			refresh: `Rafraîchi tous les ${module.refresh_days} jours`,
			chip: chipOf(module, enabled),
			stats: [
				{
					label: 'Leads inspectés',
					value: formatNumber(stats.leads_inspected),
				},
				{
					label: 'Éléments trouvés',
					value: formatNumber(stats.findings_count),
				},
				{
					label: 'Dernier passage',
					value: lastRun === null ? 'Jamais' : formatDateTime(lastRun),
				},
			],
			example:
				example === null
					? null
					: {
							text: `${example.label} : ${example.value}`,
							domain: example.domain,
							sourceUrl: example.source_url,
						},
			canTest: module.available,
			providerLink: module.available
				? providerLink(module.provider, module.provider_ready)
				: null,
		}
	})
}

export interface DryRunView {
	summary: string
	tone: StatusTone
	notice: string | null
	view: ModuleView
}

function countLabel(count: number): string {
	return count > 1 ? `${count} éléments trouvés` : `${count} élément trouvé`
}

export function dryRunView(result: DryRunResult): DryRunView {
	const notice =
		result.status === 'not_applicable'
			? (platformNotice(result.hosting_platform) ?? result.error ?? null)
			: (result.error ?? null)
	return {
		summary: `${result.domain} · ${moduleStatusLabel(result.status)} en ${formatDuration(result.duration_ms)} · ${countLabel(result.findings.length)}`,
		tone: statusTone(result.status),
		notice,
		view: moduleView(result.module, result.findings),
	}
}
