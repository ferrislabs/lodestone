import { createApiClient } from '#/api/api.client'
import { fetcher } from '#/api/api.fetch'
import { TanstackQueryApiClient } from '#/api/api.tanstack'

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
	return error instanceof Error ? error.message : 'Unexpected error'
}
