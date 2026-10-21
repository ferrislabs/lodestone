import { formatNumber, formatPercent } from '#/lib/format'

export { formatDateTime } from '#/lib/format'

export function formatScore(score: number): string {
	return formatNumber(score)
}

export const MAX_SIGNAL_STRENGTH = 100

export function formatSignalStrength(score: number): string {
	return `${formatNumber(score)} / ${MAX_SIGNAL_STRENGTH}`
}

export function formatRatio(value: number): string {
	return formatPercent(value)
}

export function scoreRatio(score: number, max: number): number {
	if (max <= 0 || score <= 0) return 0
	return Math.min(1, score / max)
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
