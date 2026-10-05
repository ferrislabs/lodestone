import { z } from 'zod'

const errorBodySchema = z.object({
	message: z.string(),
	details: z
		.array(z.object({ path: z.string(), message: z.string() }))
		.optional(),
})

export interface ParsedApiError {
	message: string
	status: number | null
	fields: Record<string, string>
}

function readStatus(error: Error): number | null {
	const status = (error as Error & { status?: unknown }).status
	return typeof status === 'number' ? status : null
}

export function parseApiError(error: unknown): ParsedApiError {
	if (!(error instanceof Error)) {
		return { message: 'Unexpected error', status: null, fields: {} }
	}
	const status = readStatus(error)
	const body = errorBodySchema.safeParse(
		(error as Error & { data?: unknown }).data,
	)
	if (!body.success) {
		return { message: error.message, status, fields: {} }
	}
	const fields: Record<string, string> = {}
	for (const detail of body.data.details ?? []) {
		if (!(detail.path in fields)) fields[detail.path] = detail.message
	}
	return { message: body.data.message, status, fields }
}
