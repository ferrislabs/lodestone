import type { CrawlSettings } from '#/pages/source-providers/lib/types'

export const ROBOTS_WARNING =
	"robots.txt peut porter l'opposition du propriétaire du site à la fouille de textes et de données (directive (UE) 2019/790, art. 4 ; CPI, art. L.122-5-3). Ignorer robots.txt engage la responsabilité de l'opérateur. Les limites de débit et l'identification du robot restent appliquées."

const CHANGED_AT = new Intl.DateTimeFormat('fr-FR', {
	dateStyle: 'long',
	timeStyle: 'short',
	timeZone: 'Europe/Paris',
})

export function ignoresRobots(settings: CrawlSettings | null): boolean {
	return settings?.robots_policy === 'ignore'
}

export function crawlPolicyLabel(ignored: boolean): string {
	return ignored
		? 'robots.txt est ignoré pour la lecture des sites.'
		: 'robots.txt est respecté pour la lecture des sites.'
}

export function crawlChangeNote(settings: CrawlSettings): string {
	if (settings.origin !== 'organisation' || !settings.updated_at) {
		return `Valeur par défaut du serveur (CRAWL_ROBOTS_POLICY = ${settings.environment_default}), jamais modifiée ici.`
	}
	const who = settings.updated_by_name ?? 'un utilisateur supprimé'
	const when = CHANGED_AT.format(new Date(settings.updated_at))
	return `Modifié par ${who} le ${when}.`
}

export function robotsIgnoredNote(count: number): string | null {
	if (count <= 0) {
		return null
	}
	return count === 1
		? '1 requête lue malgré un refus de robots.txt depuis le démarrage du serveur.'
		: `${count} requêtes lues malgré un refus de robots.txt depuis le démarrage du serveur.`
}

export function retriedNote(count: number): string | null {
	if (count <= 0) {
		return null
	}
	return count === 1
		? '1 dossier bloqué par robots.txt sera relu.'
		: `${count} dossiers bloqués par robots.txt seront relus.`
}
