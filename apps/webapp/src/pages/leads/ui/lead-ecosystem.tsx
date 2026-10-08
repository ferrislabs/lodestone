import { ModuleFindings } from '#/components/ecosystem/module-findings'
import { ModuleStatusChip } from '#/components/ecosystem/module-status-chip'
import { ProviderNotReady } from '#/components/ecosystem/provider-not-ready'
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
						{section.providerLink ? (
							<ProviderNotReady href={section.providerLink} />
						) : null}
						{section.lastRun ? (
							<span className="text-xs text-muted-foreground">
								Dernier passage : {section.lastRun}
							</span>
						) : null}
						{section.note ? (
							<p
								data-module-note={section.module}
								className={
									section.failed
										? 'text-xs text-destructive'
										: 'text-xs text-muted-foreground'
								}
							>
								{section.note}
							</p>
						) : null}
						<ModuleFindings view={section.view} empty="Aucun élément trouvé." />
					</article>
				))}
			</div>
		</section>
	)
}
