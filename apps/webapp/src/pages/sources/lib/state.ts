import { formatDateTime, formatRelative } from '#/lib/format'
import { sourceLabel } from '#/lib/labels'
import {
	CONTINUOUS_GROUP_TITLE,
	isContinuous,
} from '#/pages/sources/lib/listener'
import type {
	Source,
	SourceRun,
	SourceState,
	TriggerResult,
} from '#/pages/sources/lib/types'

export const POLL_INTERVAL_MS = 5_000
export const TRIGGER_TTL_MS = 120_000
const MAX_ERROR_CHARS = 80
export const RESULT_CLEAR_MS = 6_000

export const SOURCE_GROUPS: readonly {
	title: string
	sources: readonly string[]
}[] = [
	{
		title: 'Infrastructure',
		sources: ['certificate_transparency', 'dns_cname', 'http_probe'],
	},
	{ title: 'Marchés publics', sources: ['boamp', 'decp', 'ted'] },
	{ title: "Offres d'emploi", sources: ['france_travail', 'ats'] },
	{
		title: 'Contacts',
		sources: ['public_page', 'security_txt', 'role_guess'],
	},
	{ title: 'Qualification', sources: ['llm_qualification'] },
]

const STATE_LABELS: Readonly<Record<SourceState, string>> = {
	pending: 'En attente',
	running: 'En cours',
	failed: 'Échec',
	succeeded: 'Terminé',
	unavailable: 'Indisponible',
}

export function sourceState(source: Source): SourceState {
	switch (source.status) {
		case 'running':
		case 'failed':
		case 'succeeded':
		case 'unavailable':
			return source.status
		default:
			return 'pending'
	}
}

export function stateLabel(state: SourceState): string {
	return STATE_LABELS[state]
}

export function canRun(source: Source): boolean {
	return sourceState(source) === 'pending'
}

export function runDisabledReason(source: Source): string | null {
	switch (sourceState(source)) {
		case 'pending':
			return null
		case 'running':
			return 'La source est déjà en cours'
		case 'unavailable':
			return source.unavailable_reason === 'missing_credentials'
				? 'Identifiants manquants : la source est désactivée'
				: "Aucune tâche n'est planifiée pour cette source"
		case 'failed':
			return 'La tâche a échoué définitivement et ne sera plus planifiée'
		case 'succeeded':
			return "La tâche est terminée et n'est plus planifiée"
	}
}

export function isSettled(
	source: Source | undefined,
	triggeredAt: number,
	now: number,
): boolean {
	if (source === undefined) return true
	if (now - triggeredAt > TRIGGER_TTL_MS) return true
	const state = sourceState(source)
	if (state === 'running') return false
	if (state !== 'pending') return true
	return (
		source.next_run_at != null && Date.parse(source.next_run_at) > triggeredAt
	)
}

export function activeTriggers(
	sources: readonly Source[],
	triggeredAt: Readonly<Record<string, number>>,
	now: number,
): Set<string> {
	const active = new Set<string>()
	for (const [name, at] of Object.entries(triggeredAt)) {
		const source = sources.find((candidate) => candidate.source === name)
		if (!isSettled(source, at, now)) active.add(name)
	}
	return active
}

export function shouldPoll(
	sources: readonly Source[],
	triggeredAt: Readonly<Record<string, number>>,
	now: number,
): boolean {
	return (
		sources.some(
			(source) => isContinuous(source) || sourceState(source) === 'running',
		) || activeTriggers(sources, triggeredAt, now).size > 0
	)
}

export interface SourceGroup {
	title: string
	continuous: boolean
	sources: Source[]
}

export function groupedSources(sources: readonly Source[]): SourceGroup[] {
	const known = new Set(SOURCE_GROUPS.flatMap((group) => group.sources))
	const inOrder = SOURCE_GROUPS.flatMap((group) =>
		group.sources.flatMap((name) =>
			sources.filter((source) => source.source === name),
		),
	)
	const continuous: SourceGroup = {
		title: CONTINUOUS_GROUP_TITLE,
		continuous: true,
		sources: [
			...inOrder.filter(isContinuous),
			...sources.filter(
				(source) => !known.has(source.source) && isContinuous(source),
			),
		],
	}
	const scheduled = SOURCE_GROUPS.map((group) => ({
		title: group.title,
		continuous: false,
		sources: group.sources.flatMap((name) =>
			sources.filter(
				(source) => source.source === name && !isContinuous(source),
			),
		),
	}))
	const others = sources.filter(
		(source) => !known.has(source.source) && !isContinuous(source),
	)
	const groups = [continuous, ...scheduled]
	if (others.length > 0) {
		groups.push({ title: 'Autres', continuous: false, sources: others })
	}
	return groups.filter((group) => group.sources.length > 0)
}

export function shortError(error: string): string {
	const text = error.replace(/\s+/g, ' ').trim()
	return text.length > MAX_ERROR_CHARS
		? `${text.slice(0, MAX_ERROR_CHARS - 1)}…`
		: text
}

const RUN_STATUS_LABELS: Readonly<Record<string, string>> = {
	running: 'En cours',
	succeeded: 'Réussi',
	failed: 'Échec',
}

export interface RunSummary {
	status: string
	statusLabel: string
	failed: boolean
	counts: string | null
	error: string | null
	profile: string
	when: string
	absolute: string
}

function count(value: number, singular: string, plural: string): string {
	return `${value} ${value > 1 ? plural : singular}`
}

export function runSummary(run: SourceRun, now: number): RunSummary {
	const running = run.status === 'running'
	return {
		status: run.status,
		statusLabel: RUN_STATUS_LABELS[run.status] ?? run.status,
		failed: run.status === 'failed',
		counts: running
			? null
			: `${count(run.volume, 'élément', 'éléments')}, ${count(run.signals, 'signal', 'signaux')}`,
		error: run.error ? shortError(run.error) : null,
		profile: run.profile_name,
		when: formatRelative(run.finished_at ?? run.started_at, now),
		absolute: formatDateTime(run.finished_at ?? run.started_at),
	}
}

export function triggerResultLabel(result: TriggerResult): string {
	switch (result.kind) {
		case 'scheduled':
			return 'Planifié'
		case 'running':
			return 'Déjà en cours'
		case 'error':
			return result.message
	}
}

export function sourceTitle(source: Source): string {
	return sourceLabel(source.source)
}
