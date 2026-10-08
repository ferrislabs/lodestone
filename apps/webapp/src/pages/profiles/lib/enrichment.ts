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

export const ENRICHMENT_STEPS = ['http_probe', 'company_jobs'] as const

export type EnrichmentStepKey = (typeof ENRICHMENT_STEPS)[number]

export const ENRICHMENT_WAIT_MINUTES = 30

function stepSchema(step: EnrichmentStepKey): Schema {
	const section = resolve(
		PROFILE_SCHEMA,
		PROFILE_SCHEMA.properties?.enrichment ?? {},
	)
	return resolve(PROFILE_SCHEMA, section.properties?.[step] ?? {})
}

function stepDefaults(step: EnrichmentStepKey): JsonObject {
	return toJsonObject(
		defaultValue(PROFILE_SCHEMA, stepSchema(step), PROFILE_HINTS, [
			'enrichment',
			step,
		]),
	)
}

export function enrichmentStep(
	value: JsonObject,
	step: EnrichmentStepKey,
): JsonObject {
	const current = getIn(value, ['enrichment', step])
	return isJsonObject(current) ? current : stepDefaults(step)
}

export function stepEnabled(
	value: JsonObject,
	step: EnrichmentStepKey,
): boolean {
	return enrichmentStep(value, step).enabled === true
}

export function withStepEnabled(
	value: JsonObject,
	step: EnrichmentStepKey,
	enabled: boolean,
): JsonObject {
	const section = isJsonObject(value.enrichment) ? value.enrichment : {}
	return {
		...value,
		enrichment: {
			...section,
			[step]: { ...enrichmentStep(value, step), enabled },
		},
	}
}

export function enrichmentFieldSchema(
	step: EnrichmentStepKey,
	key: string,
): Schema {
	const property = stepSchema(step).properties?.[key]
	return { properties: property === undefined ? {} : { [key]: property } }
}

export function stepHasProblem(
	problems: readonly Problem[],
	step: EnrichmentStepKey,
	key: string,
): boolean {
	const root = `enrichment.${step}.${key}`
	return problems.some((problem) => belongsToPath(problem.path, root))
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
