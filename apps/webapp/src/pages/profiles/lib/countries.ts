export interface Country {
	code: string
	name: string
}

const LETTERS = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'

const NAMES = new Intl.DisplayNames(['fr'], {
	type: 'region',
	fallback: 'none',
})

function knownName(code: string): string | undefined {
	try {
		return NAMES.of(code)
	} catch {
		return undefined
	}
}

function isCanonical(code: string): boolean {
	try {
		return Intl.getCanonicalLocales(`und-${code}`)[0] === `und-${code}`
	} catch {
		return false
	}
}

function allCodes(): string[] {
	const codes: string[] = []
	for (const first of LETTERS) {
		for (const second of LETTERS) {
			const code = `${first}${second}`
			const name = knownName(code)
			if (name !== undefined && name !== code && isCanonical(code))
				codes.push(code)
		}
	}
	return codes
}

export const COUNTRY_CODES: readonly string[] = allCodes()

const COUNTRIES: readonly Country[] = COUNTRY_CODES.map((code) => ({
	code,
	name: knownName(code) ?? code,
})).sort((left, right) => left.name.localeCompare(right.name, 'fr'))

export function countryName(code: string): string {
	return knownName(code.toUpperCase()) ?? code
}

function fold(value: string): string {
	return value
		.normalize('NFD')
		.replace(/\p{Diacritic}/gu, '')
		.toLowerCase()
		.trim()
}

export function searchCountries(
	query: string,
	chosen: readonly string[],
	limit = 8,
): Country[] {
	const needle = fold(query)
	if (needle === '') return []
	const open = COUNTRIES.filter((country) => !chosen.includes(country.code))
	const byCode = open.filter((country) => fold(country.code) === needle)
	const byPrefix = open.filter(
		(country) =>
			fold(country.name).startsWith(needle) && !byCode.includes(country),
	)
	const ranked =
		byCode.length + byPrefix.length > 0
			? [...byCode, ...byPrefix]
			: open.filter((country) => fold(country.name).includes(needle))
	return ranked.slice(0, limit)
}

export function withCountry(codes: readonly string[], code: string): string[] {
	return codes.includes(code) ? [...codes] : [...codes, code]
}

export function withoutCountry(
	codes: readonly string[],
	code: string,
): string[] {
	return codes.filter((known) => known !== code)
}
