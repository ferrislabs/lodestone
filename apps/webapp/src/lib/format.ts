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

const RELATIVE = new Intl.RelativeTimeFormat(LOCALE, { numeric: 'auto' })
const RELATIVE_UNITS: readonly (readonly [
	Intl.RelativeTimeFormatUnit,
	number,
])[] = [
	['day', 86_400],
	['hour', 3_600],
	['minute', 60],
]
const NOW_THRESHOLD_SECONDS = 10

export function formatRelative(iso: string, now: number): string {
	const time = new Date(iso).getTime()
	if (Number.isNaN(time)) return iso
	const seconds = Math.round((time - now) / 1000)
	const magnitude = Math.abs(seconds)
	if (magnitude < NOW_THRESHOLD_SECONDS) return 'maintenant'
	for (const [unit, size] of RELATIVE_UNITS) {
		if (magnitude >= size) {
			return RELATIVE.format(Math.trunc(seconds / size), unit)
		}
	}
	return RELATIVE.format(seconds, 'second')
}
