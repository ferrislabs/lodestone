import type { ParsedApiError } from '#/lib/api-errors'

export const CONFLICT_MESSAGE =
	"Ce brouillon a changé depuis son chargement. La dernière version vient d'être rechargée : vérifiez-la, puis recommencez."

export function isConflict(error: Pick<ParsedApiError, 'status'>): boolean {
	return error.status === 409
}
