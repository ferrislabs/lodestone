import type { DraftEntry, DraftValues } from './types'

export function isEdited(draft: DraftEntry['draft']): boolean {
	return draft.original_subject != null || draft.original_body != null
}

export function valuesFromEntry(entry: DraftEntry): DraftValues {
	return { subject: entry.draft.subject, body: entry.draft.body }
}

export function previewText(text: string, max: number): string {
	const flat = text.replace(/\s+/g, ' ').trim()
	return flat.length <= max ? flat : `${flat.slice(0, max - 1)}…`
}
