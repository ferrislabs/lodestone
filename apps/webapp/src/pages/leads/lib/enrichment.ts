import {
	moduleNote,
	moduleStatusLabel,
	providerLink,
	type StatusTone,
	statusTone,
} from '#/lib/ecosystem'
import { type ModuleView, moduleView } from '#/lib/ecosystem-views'
import { formatDateTime } from '#/pages/leads/lib/format'
import type { Ecosystem, Signal } from '#/pages/leads/lib/types'

export interface ModuleSection {
	module: string
	label: string
	statusLabel: string
	tone: StatusTone
	lastRun: string | null
	note: string | null
	failed: boolean
	providerLink: string | null
	view: ModuleView
}

export function moduleSections(ecosystem: Ecosystem): ModuleSection[] {
	return ecosystem.modules.map((report) => ({
		module: report.module,
		label: report.label,
		statusLabel: moduleStatusLabel(report.status),
		tone: statusTone(report.status),
		lastRun: report.last_run ? formatDateTime(report.last_run) : null,
		note: moduleNote(report.status, report.error),
		failed: report.status === 'failed',
		providerLink: providerLink(report.provider, report.provider_ready),
		view: moduleView(report.module, report.findings),
	}))
}

export function signalTitle(signal: Signal): string {
	const host = signal.value?.trim() ?? ''
	if (signal.kind === 'certificate_seen' && host !== '') {
		return `Certificat observé pour ${host}`
	}
	return signal.sentence
}

export function signalExcerpts(signal: Signal): string[] {
	const raw: unknown = signal.raw
	if (typeof raw !== 'object' || raw === null) {
		return []
	}
	const excerpts: unknown = (raw as Record<string, unknown>).excerpts
	if (!Array.isArray(excerpts)) {
		return []
	}
	const passages = excerpts.flatMap((excerpt: unknown) =>
		typeof excerpt === 'string' && excerpt.trim() !== ''
			? [excerpt.trim()]
			: [],
	)
	return [...new Set(passages)]
}
