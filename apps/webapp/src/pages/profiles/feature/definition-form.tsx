import { SchemaForm } from '#/components/schema-form/schema-form'
import { fieldPaths, isJsonObject, type JsonObject } from '#/lib/schema-form'
import { PROFILE_HINTS } from '#/pages/profiles/lib/form-hints'
import { PROFILE_SCHEMA } from '#/pages/profiles/lib/profile-schema'
import type { Problem } from '#/pages/profiles/lib/types'

export function unmatchedProblems(
	value: JsonObject,
	problems: readonly Problem[],
): Problem[] {
	const known = fieldPaths(PROFILE_SCHEMA, PROFILE_SCHEMA, value)
	return problems.filter((problem) => !known.has(problem.path))
}

export function renderDefinitionForm(
	value: JsonObject,
	problems: readonly Problem[],
	onChange: (next: JsonObject) => void,
) {
	return (
		<SchemaForm
			context={{
				root: PROFILE_SCHEMA,
				hints: PROFILE_HINTS,
				value,
				problems,
				onChange: (next) => {
					if (isJsonObject(next)) onChange(next)
				},
			}}
		/>
	)
}
