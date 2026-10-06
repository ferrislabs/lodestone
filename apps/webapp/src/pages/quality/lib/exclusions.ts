import type { ExclusionCandidate } from '#/pages/quality/lib/types'

const LISTS: Readonly<Record<string, string>> = {
	deja_client: 'Clients actuels',
	concurrent: 'Concurrents',
	editeur_integrateur: 'Éditeurs et intégrateurs du domaine',
	hebergeur_agence: 'Éditeurs et intégrateurs du domaine',
}

export const EXCLUSION_PROMPT_HINT =
	"Collez le texte copié dans la liste correspondante du prompt de l'agent « Exclusions », puis enregistrez le profil. Rien n'est modifié automatiquement."

export function exclusionListName(reason: string): string {
	return LISTS[reason] ?? 'Éditeurs et intégrateurs du domaine'
}

export function clipboardText(
	candidate: Pick<ExclusionCandidate, 'domain' | 'legal_name'>,
): string {
	const name = candidate.legal_name?.trim()
	return name ? `${name}, ${candidate.domain}` : candidate.domain
}

export function exclusionOptionLabel(row: {
	agent_name: string
	label: string
}): string {
	return `${row.agent_name} : ${row.label}`
}
