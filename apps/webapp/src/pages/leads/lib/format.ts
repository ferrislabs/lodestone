export function formatScore(score: number): string {
	return Number.isInteger(score) ? String(score) : score.toFixed(1)
}

export function formatRatio(value: number): string {
	return `${Math.round(value * 100)}%`
}

export function scoreRatio(score: number, max: number): number {
	if (max <= 0 || score <= 0) return 0
	return Math.min(1, score / max)
}

export function formatDateTime(iso: string): string {
	const date = new Date(iso)
	if (Number.isNaN(date.getTime())) return iso
	return date.toLocaleString('en-GB', {
		dateStyle: 'medium',
		timeStyle: 'short',
	})
}

export function externalHref(value: string): string | null {
	try {
		const url = new URL(value)
		return url.protocol === 'http:' || url.protocol === 'https:'
			? url.href
			: null
	} catch {
		return null
	}
}
