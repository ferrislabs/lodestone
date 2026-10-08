import { FindingList } from '#/components/ecosystem/finding-list'
import { ModuleStatusChip } from '#/components/ecosystem/module-status-chip'
import { Badge } from '#/components/ui/badge'
import { platformNotice } from '#/lib/ecosystem'
import {
	enrichmentStatusLabel,
	moduleSections,
	waitingBadge,
} from '#/pages/leads/lib/enrichment'
import type { Ecosystem, Enrichment } from '#/pages/leads/lib/types'

interface LeadEcosystemProps {
	ecosystem: Ecosystem
	enrichment: Enrichment
}

export function LeadEcosystem({ ecosystem, enrichment }: LeadEcosystemProps) {
	const sections = moduleSections(ecosystem)
	const waiting = waitingBadge(enrichment)
	const notice = platformNotice(ecosystem.hosting_platform)
	return (
		<section
			data-section="ecosystem"
			aria-label="Écosystème"
			className="flex flex-col gap-3 rounded-lg border p-4"
		>
			<div className="flex flex-wrap items-center gap-3">
				<h2 className="text-sm font-semibold">Écosystème</h2>
				<span className="text-xs text-muted-foreground">
					{enrichmentStatusLabel(enrichment.status)}
				</span>
				{waiting ? (
					<Badge
						variant="outline"
						data-waiting={waiting.reason}
						className="border-amber-200 bg-amber-50 text-amber-700"
					>
						{waiting.detail
							? `${waiting.label} ${waiting.detail}`
							: waiting.label}
					</Badge>
				) : null}
			</div>
			{notice ? (
				<p
					data-notice="hosting-platform"
					className="rounded-md border border-sky-200 bg-sky-50 px-3 py-2 text-sm text-sky-800"
				>
					{notice}
				</p>
			) : null}
			<div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
				{sections.map((section) => (
					<article
						key={section.module}
						data-module={section.module}
						aria-label={section.label}
						className="flex min-w-0 flex-col gap-2 rounded-md border p-3"
					>
						<div className="flex flex-wrap items-center justify-between gap-2">
							<h3 className="text-sm font-medium">{section.label}</h3>
							<ModuleStatusChip
								label={section.statusLabel}
								tone={section.tone}
							/>
						</div>
						{section.lastRun ? (
							<span className="text-xs text-muted-foreground">
								Dernier passage : {section.lastRun}
							</span>
						) : null}
						{section.error ? (
							<p className="text-xs text-destructive">{section.error}</p>
						) : null}
						<FindingList rows={section.rows} empty="Aucun élément trouvé." />
					</article>
				))}
			</div>
		</section>
	)
}
