const LOCALE = 'fr-FR'

const NUMBER = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: 1 })
const PERCENT = new Intl.NumberFormat(LOCALE, {
	style: 'percent',
	maximumFractionDigits: 0,
})
const DATE_TIME = new Intl.DateTimeFormat(LOCALE, {
	dateStyle: 'medium',
	timeStyle: 'short',
})

export function formatNumber(value: number): string {
	return NUMBER.format(value)
}

export function formatPercent(ratio: number): string {
	return PERCENT.format(ratio)
}

export function formatDateTime(iso: string): string {
	const date = new Date(iso)
	if (Number.isNaN(date.getTime())) return iso
	return DATE_TIME.format(date)
}
