import { z } from 'zod'

const CONNECTED_FLAGS = [1, '1', true]

export const providerSearchSchema = z.object({
	connected: z
		.unknown()
		.transform((value) => CONNECTED_FLAGS.includes(value as never) || undefined)
		.optional(),
	error: z.string().max(64).optional().catch(undefined),
})

export type ProviderSearch = z.infer<typeof providerSearchSchema>

export const catalogSearchSchema = z.object({
	category: z.string().max(64).optional().catch(undefined),
	status: z.string().max(32).optional().catch(undefined),
	q: z.string().max(100).optional().catch(undefined),
})

export type CatalogSearch = z.infer<typeof catalogSearchSchema>
