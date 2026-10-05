const SEPARATORS = /[,;\n\r]+/

export function splitTags(text: string): string[] {
	return text
		.split(SEPARATORS)
		.map((part) => part.trim())
		.filter((part) => part !== '')
}

export interface MergedTags {
	next: string[]
	added: number
	duplicates: number
}

export function mergeTags(
	values: readonly string[],
	incoming: readonly string[],
): MergedTags {
	const seen = new Set(values.map((value) => value.toLowerCase()))
	const next = [...values]
	let duplicates = 0
	for (const candidate of incoming) {
		const trimmed = candidate.trim()
		if (trimmed === '') continue
		const key = trimmed.toLowerCase()
		if (seen.has(key)) {
			duplicates += 1
			continue
		}
		seen.add(key)
		next.push(trimmed)
	}
	return { next, added: next.length - values.length, duplicates }
}
