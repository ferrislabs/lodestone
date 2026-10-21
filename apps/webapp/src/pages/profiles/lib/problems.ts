import { z } from 'zod'

import { translateApiMessage } from '#/lib/api-errors'
import type { DefinitionParse, Problem } from '#/pages/profiles/lib/types'

const detailsSchema = z.object({
	message: z.string().optional(),
	details: z
		.array(z.object({ path: z.string(), message: z.string() }))
		.nullish(),
})

export function problemsFromError(error: unknown): Problem[] {
	if (!(error instanceof Error)) {
		return [{ path: '$', message: translateApiMessage('Unexpected error') }]
	}
	const body = detailsSchema.safeParse(
		(error as Error & { data?: unknown }).data,
	)
	const details = body.success ? (body.data.details ?? []) : []
	if (details.length > 0) {
		return details.map((detail) => ({
			path: detail.path,
			message: translateApiMessage(detail.message),
		}))
	}
	return [{ path: '$', message: translateApiMessage(error.message) }]
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
		const reason = error instanceof Error ? error.message : 'JSON invalide'
		return {
			ok: false,
			problems: [{ path: '$', message: `JSON mal formé : ${reason}` }],
		}
	}
	if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
		return {
			ok: false,
			problems: [
				{ path: '$', message: translateApiMessage('must be a JSON object') },
			],
		}
	}
	return { ok: true, value: Object.fromEntries(Object.entries(parsed)) }
}
