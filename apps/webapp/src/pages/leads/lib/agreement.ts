import { formatNumber } from '#/lib/format'

export function agreementLabel(
	agreement: number | null | undefined,
	samples: number,
): string | null {
	if (agreement == null || !Number.isFinite(agreement)) return null
	if (samples >= 1 && samples <= 5) {
		const agreeing = Math.round(agreement * samples)
		if (Math.abs(agreeing / samples - agreement) < 0.005) {
			return `Accord : ${agreeing}/${samples}`
		}
	}
	return `Accord : ${formatNumber(Math.round(agreement * 1000) / 10)} %`
}

export function escalationLabel(
	providerName: string | null | undefined,
	model: string,
): string {
	const target = providerName || model
	return target ? `Escaladé vers ${target}` : 'Escaladé'
}
