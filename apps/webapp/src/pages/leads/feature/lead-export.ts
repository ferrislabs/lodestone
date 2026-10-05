import { getApiUrl } from '#/api/lodestone'
import type { LeadsQuery } from '#/pages/leads/lib/search'
import { authStore } from '#/store/auth.store'

export type ExportFormat = 'json' | 'csv'

export function exportUrl(
	baseUrl: string,
	profileId: string,
	format: ExportFormat,
	query: LeadsQuery,
): string {
	const params = new URLSearchParams({ format })
	for (const [key, value] of Object.entries(query)) {
		if (value !== undefined) params.set(key, String(value))
	}
	const base = baseUrl.replace(/\/+$/, '')
	return `${base}/api/v1/profiles/${encodeURIComponent(profileId)}/leads/export?${params.toString()}`
}

function filenameFrom(header: string | null, fallback: string): string {
	const match = header?.match(/filename="?([^";]+)"?/i)
	return match?.[1] ?? fallback
}

async function failureMessage(response: Response): Promise<string> {
	try {
		const body: { message?: string } = await response.json()
		if (body.message) return body.message
	} catch {
		return `HTTP ${response.status}`
	}
	return `HTTP ${response.status}`
}

export async function downloadLeadExport(
	profileId: string,
	format: ExportFormat,
	query: LeadsQuery,
): Promise<void> {
	const headers = new Headers()
	const token = authStore.state.accessToken
	if (token) headers.set('Authorization', `Bearer ${token}`)

	const response = await fetch(
		exportUrl(getApiUrl(), profileId, format, query),
		{ headers },
	)
	if (!response.ok) throw new Error(await failureMessage(response))

	const blob = await response.blob()
	const filename = filenameFrom(
		response.headers.get('content-disposition'),
		`leads.${format}`,
	)
	const objectUrl = URL.createObjectURL(blob)
	const anchor = document.createElement('a')
	anchor.href = objectUrl
	anchor.download = filename
	document.body.appendChild(anchor)
	anchor.click()
	anchor.remove()
	URL.revokeObjectURL(objectUrl)
}
