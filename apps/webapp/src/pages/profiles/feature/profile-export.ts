import { getApiUrl } from '#/api/lodestone'
import { translateApiMessage } from '#/lib/api-errors'
import { exportFilename } from '#/pages/profiles/lib/definition'
import { authStore } from '#/store/auth.store'

function filenameFrom(header: string | null, fallback: string): string {
	const match = header?.match(/filename="?([^";]+)"?/i)
	return match?.[1] ?? fallback
}

async function failureMessage(response: Response): Promise<string> {
	try {
		const body: { message?: string } = await response.json()
		return translateApiMessage(body.message ?? `HTTP ${response.status}`)
	} catch {
		return translateApiMessage(`HTTP ${response.status}`)
	}
}

export async function downloadProfileExport(
	profileId: string,
	name: string,
): Promise<void> {
	const headers = new Headers()
	const token = authStore.state.accessToken
	if (token) headers.set('Authorization', `Bearer ${token}`)

	const base = getApiUrl().replace(/\/+$/, '')
	const response = await fetch(
		`${base}/api/v1/profiles/${encodeURIComponent(profileId)}/export`,
		{ headers },
	)
	if (!response.ok) throw new Error(await failureMessage(response))

	const blob = await response.blob()
	const filename = filenameFrom(
		response.headers.get('content-disposition'),
		exportFilename(name),
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
