import { createApiClient } from '#/api/api.client'
import { fetcher } from '#/api/api.fetch'
import { TanstackQueryApiClient } from '#/api/api.tanstack'
import { parseApiError } from '#/lib/api-errors'

const client = createApiClient({ fetch: fetcher })
const queries = new TanstackQueryApiClient(client)

export function getApiUrl(): string {
	return typeof window === 'undefined' ? '' : (window.apiUrl ?? '')
}

export function lodestoneApi(): TanstackQueryApiClient {
	client.setBaseUrl(getApiUrl())
	return queries
}

export function errorMessage(error: unknown): string {
	return parseApiError(error).message
}
