import { formatNumber, formatPercent } from '#/lib/format'
import type { ProfileQuality } from '#/pages/quality/lib/types'

export function formatRatio(ratio: number | null | undefined): string {
	return ratio == null ? '—' : formatPercent(ratio)
}

export function formatScore(score: number | null | undefined): string {
	return score == null
		? '—'
		: `${formatNumber(Math.round(score * 1000) / 10)} %`
}

export function formatSeparation(
	separation: number | null | undefined,
): string {
	if (separation == null) return '—'
	const points = Math.round(separation * 1000) / 10
	return `${points > 0 ? '+' : ''}${formatNumber(points)} pts`
}

export function insufficientMessage(suggestion: {
	labelled: number
	required: number
}): string {
	return `Pas assez de décisions (${suggestion.labelled}/${suggestion.required})`
}

export function reliableSuggestion(profile: ProfileQuality): string {
	const { reliable_from } = profile.suggestion
	if (reliable_from == null) {
		return 'Aucun seuil « Fiable » suggéré : aucun lead n’a été validé.'
	}
	return `Seuil « Fiable » suggéré : ${reliable_from} % (actuel ${profile.current.reliable_from} %)`
}

export function discardSuggestion(profile: ProfileQuality): string {
	const { discard_below } = profile.suggestion
	if (discard_below == null) {
		return 'Aucun seuil « Écarté » suggéré : trop de leads validés parmi les plus bas.'
	}
	return `Seuil « Écarté » suggéré : ${discard_below} % (actuel ${profile.current.discard_below} %)`
}

export function bucketLabel(bucket: { from: number; to: number }): string {
	return `${bucket.from}–${bucket.to} %`
}

export function hasDecisions(quality: {
	decisions: {
		validated: number
		rejected: number
		drafts_approved: number
		drafts_rejected: number
	}
}): boolean {
	const { decisions } = quality
	return (
		decisions.validated +
			decisions.rejected +
			decisions.drafts_approved +
			decisions.drafts_rejected >
		0
	)
}

export function histogramMax(
	buckets: readonly { validated: number; rejected: number }[],
): number {
	return buckets.reduce(
		(max, bucket) => Math.max(max, bucket.validated, bucket.rejected),
		0,
	)
}

export function barHeight(count: number, max: number): number {
	return max === 0 ? 0 : Math.round((count / max) * 100)
}
