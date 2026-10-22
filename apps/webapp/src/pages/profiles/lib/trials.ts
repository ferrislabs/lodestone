import { leadStatusLabel } from '#/pages/leads/lib/labels'
import type {
	Trial,
	TrialDetail,
	TrialLead,
	VersionSummary,
} from '#/pages/profiles/lib/types'

export const SAMPLE_SIZE_MIN = 1
export const SAMPLE_SIZE_MAX = 50
export const SAMPLE_SIZE_DEFAULT = 20
export const BUDGET_CENTS_MIN = 1
export const BUDGET_CENTS_MAX = 2000
export const BUDGET_CENTS_DEFAULT = 200
export const TRIAL_POLL_MS = 3_000

const SAMPLE_SIZE_MESSAGE = `La taille de l'échantillon doit être un entier de ${SAMPLE_SIZE_MIN} à ${SAMPLE_SIZE_MAX}`
const BUDGET_MESSAGE = `Le budget doit être un entier de ${BUDGET_CENTS_MIN} à ${BUDGET_CENTS_MAX} centimes`

const STATUS_LABELS: Readonly<Record<string, string>> = {
	pending: 'En attente',
	running: 'En cours',
	done: 'Terminé',
	failed: 'Échec',
	budget_exhausted: 'Budget atteint',
}

const EUROS = new Intl.NumberFormat('fr-FR', {
	minimumFractionDigits: 2,
	maximumFractionDigits: 2,
})

const WHOLE_NUMBER = /^\d+$/

export interface SummaryRow {
	key: string
	label: string
	current: string
	trial: string
}

export type TrialLimits =
	| { ok: true; sampleSize: number; budgetCents: number }
	| { ok: false; message: string }

export function trialStatusLabel(status: string): string {
	return STATUS_LABELS[status] ?? status
}

export function isTrialLive(status: string): boolean {
	return status === 'pending' || status === 'running'
}

export function formatCents(cents: number): string {
	return `${EUROS.format(cents / 100)} €`
}

export function formatTrialCost(trial: Trial): string {
	return `${formatCents(trial.spent_cents)} sur ${formatCents(trial.budget_cents)}`
}

export function trialTitle(trial: Trial): string {
	return `Révision ${trial.base_revision} + brouillon`
}

export function formatTrialProgress(trial: Trial): string {
	return `${trial.leads_tried}/${trial.sample_size} leads`
}

export function formatDiscarded(summary: VersionSummary): string {
	return `${summary.rejected_discarded}/${summary.rejected}`
}

export function formatPrecision(precision: number | null | undefined): string {
	return precision == null ? '—' : `${Math.round(precision * 100)} %`
}

export function changedText(changed: number): string {
	if (changed === 0) return 'Aucun lead ne change de verdict'
	if (changed === 1) return '1 lead change de verdict'
	return `${changed} leads changent de verdict`
}

export function errorsText(errors: number): string {
	return errors === 1 ? '1 lead en erreur' : `${errors} leads en erreur`
}

export function summaryRows(summary: TrialDetail['summary']): SummaryRow[] {
	const { current, trial } = summary
	return [
		{
			key: 'reliable',
			label: 'Fiable',
			current: String(current.reliable),
			trial: String(trial.reliable),
		},
		{
			key: 'validated',
			label: 'dont validés',
			current: String(current.reliable_validated),
			trial: String(trial.reliable_validated),
		},
		{
			key: 'precision',
			label: 'Précision',
			current: formatPrecision(current.precision),
			trial: formatPrecision(trial.precision),
		},
		{
			key: 'discarded',
			label: 'Rejetés classés écartés',
			current: formatDiscarded(current),
			trial: formatDiscarded(trial),
		},
	]
}

export function decisionLabel(label: string): string {
	if (label === 'validated' || label === 'rejected') {
		return leadStatusLabel(label)
	}
	return label
}

export function leadChanged(lead: TrialLead): boolean {
	if (lead.error != null) return false
	return (lead.trial_verdict ?? null) !== (lead.current_verdict ?? null)
}

export function changedLeads(leads: readonly TrialLead[]): TrialLead[] {
	return leads.filter(leadChanged)
}

function wholeNumberIn(text: string, min: number, max: number): number | null {
	const trimmed = text.trim()
	if (!WHOLE_NUMBER.test(trimmed)) return null
	const value = Number(trimmed)
	return value >= min && value <= max ? value : null
}

export function parseTrialLimits(sample: string, budget: string): TrialLimits {
	const sampleSize = wholeNumberIn(sample, SAMPLE_SIZE_MIN, SAMPLE_SIZE_MAX)
	if (sampleSize === null) return { ok: false, message: SAMPLE_SIZE_MESSAGE }
	const budgetCents = wholeNumberIn(budget, BUDGET_CENTS_MIN, BUDGET_CENTS_MAX)
	if (budgetCents === null) return { ok: false, message: BUDGET_MESSAGE }
	return { ok: true, sampleSize, budgetCents }
}
