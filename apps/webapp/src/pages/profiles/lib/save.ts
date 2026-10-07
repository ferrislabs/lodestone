import { translateApiMessage } from '#/lib/api-errors'
import { type JsonObject, validateValue } from '#/lib/schema-form'
import { validateAgents } from '#/pages/profiles/lib/agents'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import { validateQualification } from '#/pages/profiles/lib/qualification'
import { validateScoringAgents } from '#/pages/profiles/lib/scoring-agents'
import type { Problem } from '#/pages/profiles/lib/types'

export type SaveAttempt =
	| { ok: true; value: JsonObject }
	| { ok: false; problems: Problem[] }

export function prepareSave(value: JsonObject): SaveAttempt {
	const problems = [
		...validateValue(PROFILE_SCHEMA, PROFILE_SCHEMA, value).map((problem) => ({
			...problem,
			message: translateApiMessage(problem.message),
		})),
		...validateAgents(value),
		...validateQualification(value),
		...validateScoringAgents(value),
	]
	return problems.length === 0 ? { ok: true, value } : { ok: false, problems }
}
