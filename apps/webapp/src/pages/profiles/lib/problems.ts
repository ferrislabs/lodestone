import { z } from 'zod'

import type { DefinitionParse, Problem } from '#/pages/profiles/lib/types'

const detailsSchema = z.object({
	message: z.string().optional(),
	details: z
		.array(z.object({ path: z.string(), message: z.string() }))
		.nullish(),
})

export function problemsFromError(error: unknown): Problem[] {
	if (!(error instanceof Error)) {
		return [{ path: '$', message: 'Unexpected error' }]
	}
	const body = detailsSchema.safeParse(
		(error as Error & { data?: unknown }).data,
	)
	const details = body.success ? (body.data.details ?? []) : []
	if (details.length > 0) return details
	return [{ path: '$', message: error.message }]
}

export function problemsForPath(
	problems: readonly Problem[],
	path: string,
): Problem[] {
	return problems.filter((problem) => problem.path === path)
}

export function problemsOutsidePath(
	problems: readonly Problem[],
	path: string,
): Problem[] {
	return problems.filter((problem) => problem.path !== path)
}

export function formatProblem(problem: Problem): string {
	return `${problem.path}: ${problem.message}`
}

export function parseDefinition(text: string): DefinitionParse {
	let parsed: unknown
	try {
		parsed = JSON.parse(text)
	} catch (error) {
		const reason = error instanceof Error ? error.message : 'invalid JSON'
		return {
			ok: false,
			problems: [{ path: '$', message: `malformed JSON: ${reason}` }],
		}
	}
	if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
		return {
			ok: false,
			problems: [{ path: '$', message: 'must be a JSON object' }],
		}
	}
	return { ok: true, value: Object.fromEntries(Object.entries(parsed)) }
}
