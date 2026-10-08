import { formatNumber, formatRelative } from '#/lib/format'
import { pluralize } from '#/lib/labels'
import type {
	Listener,
	ListenerSummary,
	Source,
} from '#/pages/sources/lib/types'

export const CONTINUOUS_GROUP_TITLE = 'Écoute continue'

export const CONTINUOUS_AGENT_KINDS: readonly string[] = ['certificates', 'dns']

export type ListenerTone = 'active' | 'standby' | 'inactive' | 'error' | 'stale'

export const STALE_EXPLANATION =
	"L'instance qui écoutait ne répond plus ; une autre prendra le relais"

export interface ListenerBadge {
	label: string
	tone: ListenerTone
}

const LISTENER_BADGES: Readonly<Record<string, ListenerBadge>> = {
	listening: { label: 'En écoute', tone: 'active' },
	idle: { label: 'En écoute', tone: 'active' },
	standby: { label: 'En veille (autre instance)', tone: 'standby' },
	idle_no_agent: { label: 'Inactif (aucun agent)', tone: 'inactive' },
	error: { label: 'Erreur', tone: 'error' },
	stopped: { label: 'Arrêté', tone: 'inactive' },
	stale: { label: 'Sans nouvelles', tone: 'stale' },
}

export function isContinuous(source: Source): boolean {
	return source.mode === 'continuous'
}

export function isContinuousKind(kind: string): boolean {
	return CONTINUOUS_AGENT_KINDS.includes(kind)
}

export function listenerBadge(state: string): ListenerBadge {
	return LISTENER_BADGES[state] ?? { label: state, tone: 'inactive' }
}

export function backlogText(source: string, backlog: number): string {
	const amount = formatNumber(backlog)
	switch (source) {
		case 'certificate_transparency':
			return `${amount} ${pluralize(backlog, 'tuile', 'tuiles')} de retard`
		case 'qualification':
			return `${amount} ${pluralize(backlog, 'lead à qualifier', 'leads à qualifier')}`
		case 'company_research':
			return `${amount} ${pluralize(backlog, 'domaine à rechercher', 'domaines à rechercher')}`
		case 'ecosystem_inspection':
			return `${amount} ${pluralize(backlog, 'module à passer', 'modules à passer')}`
		case 'dns_cname':
		case 'http_probe':
			return `${amount} ${pluralize(backlog, 'hôte', 'hôtes')} à sonder`
		default:
			return `${amount} ${pluralize(backlog, 'cible', 'cibles')} de contact`
	}
}

export function lastUpdateText(
	updatedAt: string | null | undefined,
	now: number,
): string {
	return updatedAt
		? `dernière mise à jour ${formatRelative(updatedAt, now)}`
		: 'aucune mise à jour'
}

export function lastActivityText(
	lastActivityAt: string | null | undefined,
	now: number,
): string {
	return lastActivityAt
		? formatRelative(lastActivityAt, now)
		: 'Aucune activité'
}

export function totalsText(listener: Listener): string {
	const items = formatNumber(listener.items_total)
	const signals = formatNumber(listener.signals_total)
	return `${items} ${pluralize(listener.items_total, 'élément traité', 'éléments traités')}, ${signals} ${pluralize(listener.signals_total, 'signal', 'signaux')}`
}

export function agentListenerLine(
	summary: ListenerSummary | undefined,
	now: number,
): string {
	if (!summary) return 'Écoute continue'
	const badge = listenerBadge(summary.state)
	if (badge.tone === 'stale') return `${badge.label} · ${STALE_EXPLANATION}`
	if (badge.tone !== 'active') return badge.label
	if (!summary.last_activity_at) {
		return `${badge.label} · aucune activité pour le moment`
	}
	return `${badge.label} · dernière activité ${formatRelative(summary.last_activity_at, now)}`
}

export function listenerSummaries(
	sources: readonly Source[],
): Record<string, ListenerSummary> {
	const summaries: Record<string, ListenerSummary> = {}
	for (const source of sources) {
		if (source.listener) {
			summaries[source.source] = {
				state: source.listener.state,
				last_activity_at: source.listener.last_activity_at,
			}
		}
	}
	return summaries
}
