import type { Schemas } from '#/api/api.client'
import { sourceLabel } from '#/lib/labels'
import { formatClock } from '#/pages/leads/lib/evaluation'
import { formatDateTime } from '#/pages/leads/lib/format'

export type LeadTimeline = Schemas.DataEnvelope_LeadTimelineDto['data']
export type TimelineRound = Schemas.TimelineRoundDto
export type TimelineEvent = Schemas.TimelineEventDto
export type TimelineStep = Schemas.TimelineStepDto
export type EventStatus = Schemas.EventStatusDto
export type EventActor = Schemas.EventActorDto
export type InFlight = Schemas.InFlightDto
export type PipelineStage = Schemas.PipelineStageDto
export type StageStatus = Schemas.StageStatusDto
export type Stage = Schemas.StageDto

export const FAST_POLL_MS = 5_000
export const SLOW_POLL_MS = 30_000
export const NEAR_MS = 2 * 60_000

const STEP_LABELS: Readonly<Record<TimelineStep, string>> = {
	detection: 'Détection',
	exclusion: 'Exclusion',
	inspection: 'Inspection',
	qualification: 'Qualification',
	escalation: 'Escalade',
	scoring: 'Scoring',
	combination: 'Combinaison',
	aggregation: 'Synthèse',
	requalification: 'Requalification',
	stop: 'Arrêt',
	decision: 'Décision',
}

const STEP_ORDER: readonly TimelineStep[] = [
	'detection',
	'exclusion',
	'inspection',
	'qualification',
	'escalation',
	'scoring',
	'combination',
	'aggregation',
	'requalification',
	'stop',
	'decision',
]

const EVENT_STATUS_LABELS: Readonly<Record<EventStatus, string>> = {
	started: 'En cours',
	succeeded: 'Réussi',
	failed: 'Échec',
	skipped: 'Ignoré',
	waiting: 'En attente',
}

const STAGE_STATUS_LABELS: Readonly<Record<StageStatus, string>> = {
	done: 'Terminé',
	running: 'En cours',
	waiting: 'En attente',
	blocked: 'Bloqué',
	skipped: 'Sans objet',
}

const KIND_LABELS: Readonly<Record<string, string>> = {
	signal: 'Signal détecté',
	excluded: 'Lead exclu',
	exclusion_lifted: 'Exclusion levée',
	module: "Module d'inspection",
	agent: 'Agent',
	sample_invalid: 'Échantillon invalide',
	call_failed: "Échec de l'appel au modèle",
	escalated: 'Escalade déclenchée',
	combined: 'Verdict combiné',
	summary: 'Synthèse',
	summary_failed: 'Échec de la synthèse',
	draft: 'Brouillon rédigé',
	quality_warnings: 'Alertes qualité',
	planned: 'Requalification planifiée',
	reevaluation_requested: 'Réévaluation demandée',
	agent_stopped: 'Agent arrêté',
	status_changed: 'Statut modifié',
	round_started: 'Évaluation démarrée',
}

export function stepLabel(step: TimelineStep): string {
	return STEP_LABELS[step]
}

export function eventStatusLabel(status: EventStatus): string {
	return EVENT_STATUS_LABELS[status]
}

export function stageStatusLabel(status: StageStatus): string {
	return STAGE_STATUS_LABELS[status]
}

export function eventLabel(event: TimelineEvent): string {
	return KIND_LABELS[event.kind] ?? event.kind
}

export function actorText(actor: EventActor): string | null {
	const who =
		actor.agent_name ??
		actor.agent_id ??
		actor.module_label ??
		actor.module ??
		(actor.source ? sourceLabel(actor.source) : null)
	const model = actor.model
		? actor.provider && actor.provider !== actor.model
			? `${actor.model} (${actor.provider})`
			: actor.model
		: null
	const parts = [who, model].filter(
		(part): part is string => part !== null && part !== undefined,
	)
	return parts.length > 0 ? parts.join(' · ') : null
}

export function formatDuration(ms: number | null | undefined): string | null {
	if (ms === null || ms === undefined) return null
	if (ms < 1_000) return `${ms} ms`
	const seconds = ms / 1_000
	if (seconds < 60) return `${seconds.toFixed(1).replace('.', ',')} s`
	const minutes = Math.floor(seconds / 60)
	const rest = Math.round(seconds % 60)
	return rest > 0 ? `${minutes} min ${rest} s` : `${minutes} min`
}

function sourcesText(round: TimelineRound): string {
	const labels = round.source_labels.map(sourceLabel)
	return labels.length > 0 ? ` (${labels.join(', ')})` : ''
}

export function roundTitle(round: TimelineRound): string {
	const name = `Évaluation n°${round.number}`
	switch (round.trigger) {
		case 'pending':
			return `${name} — en préparation${sourcesText(round)}`
		case 'detection':
			return `${name} — déclenchée par : détection${sourcesText(round)}`
		case 'new_information':
			return `${name} — déclenchée par : nouvelles informations${sourcesText(round)}`
		case 'manual':
			return `${name} — déclenchée par : réévaluation manuelle`
		case 'requalification':
			return `${name} — déclenchée par : requalification`
	}
}

export function presentSteps(rounds: readonly TimelineRound[]): TimelineStep[] {
	const present = new Set<TimelineStep>()
	for (const round of rounds) {
		for (const event of round.events) present.add(event.step)
	}
	return STEP_ORDER.filter((step) => present.has(step))
}

export function filterRounds(
	rounds: readonly TimelineRound[],
	steps: readonly TimelineStep[],
): TimelineRound[] {
	if (steps.length === 0) return [...rounds]
	return rounds
		.map((round) => ({
			...round,
			events: round.events.filter((event) => steps.includes(event.step)),
		}))
		.filter((round) => round.events.length > 0)
}

export function toggleStep(
	steps: readonly TimelineStep[],
	step: TimelineStep,
): TimelineStep[] {
	return steps.includes(step)
		? steps.filter((known) => known !== step)
		: [...steps, step]
}

export function timelinePollMs(
	timeline: LeadTimeline | null | undefined,
	now: number,
): number {
	if (!timeline) return SLOW_POLL_MS
	if (timeline.in_flight.length > 0) return FAST_POLL_MS
	const stages = timeline.pipeline.stages
	if (stages.some((stage) => stage.status === 'running')) return FAST_POLL_MS
	const near = stages.some((stage) => {
		if (!stage.next_at) return false
		const at = new Date(stage.next_at).getTime()
		return !Number.isNaN(at) && at - now <= NEAR_MS
	})
	return near ? FAST_POLL_MS : SLOW_POLL_MS
}

export function nextText(stage: PipelineStage, now: number): string | null {
	if (!stage.next_action) return null
	if (!stage.next_at) return `Prochaine étape : ${stage.next_action}`
	const at = new Date(stage.next_at).getTime()
	if (Number.isNaN(at)) return `Prochaine étape : ${stage.next_action}`
	if (at <= now) return `Prochaine étape : ${stage.next_action} (imminent)`
	const when =
		at - now < 24 * 3_600_000
			? `à ${formatClock(stage.next_at)}`
			: `le ${formatDateTime(stage.next_at)}`
	return `Prochaine étape : ${stage.next_action} ${when}`
}

export function currentStage(timeline: LeadTimeline): PipelineStage | null {
	const current = timeline.pipeline.current
	if (!current) return null
	return (
		timeline.pipeline.stages.find((stage) => stage.stage === current) ?? null
	)
}

export function detailText(event: TimelineEvent): string | null {
	const entries = Object.entries(event.detail).filter(
		([key]) => key !== 'reconstructed',
	)
	if (entries.length === 0) return null
	return JSON.stringify(Object.fromEntries(entries), null, 2)
}
