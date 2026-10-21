import type { ReactNode } from 'react'

import type { FormContext } from '#/components/schema-form/context'
import { ObjectFields } from '#/components/schema-form/fields'
import { Switch } from '#/components/ui/switch'
import type { JsonObject } from '#/lib/schema-form'
import { cn } from '#/lib/utils'
import {
	type EnrichmentStepKey,
	enrichmentFieldSchema,
} from '#/pages/profiles/lib/enrichment'

interface EnrichmentEditorProps {
	context: FormContext
	probe: JsonObject
	jobs: JsonObject
	keywordUnion: readonly string[]
	advancedOpen: boolean
	onEnabled: (step: EnrichmentStepKey, enabled: boolean) => void
}

const CARD = 'flex min-w-0 flex-col gap-4 rounded-xl border bg-card p-5'

interface StepCardProps {
	title: string
	description: string
	switchLabel?: string
	enabled?: boolean
	onEnabled?: (enabled: boolean) => void
	children: ReactNode
}

function StepCard({
	title,
	description,
	switchLabel,
	enabled,
	onEnabled,
	children,
}: StepCardProps) {
	return (
		<section aria-label={title} className={CARD}>
			<div className="flex items-start justify-between gap-3">
				<div className="flex flex-col gap-1">
					<h2 className="text-base font-semibold">{title}</h2>
					<p className="text-sm text-muted-foreground">{description}</p>
				</div>
				{switchLabel && onEnabled ? (
					<Switch
						aria-label={switchLabel}
						checked={enabled === true}
						onCheckedChange={onEnabled}
					/>
				) : null}
			</div>
			{children}
		</section>
	)
}

export function EnrichmentEditor({
	context,
	probe,
	jobs,
	keywordUnion,
	advancedOpen,
	onEnabled,
}: EnrichmentEditorProps) {
	const field = (step: EnrichmentStepKey, key: string, value: JsonObject) => (
		<ObjectFields
			context={context}
			schema={enrichmentFieldSchema(step, key)}
			path={['enrichment', step]}
			value={value}
		/>
	)
	const probeEnabled = probe.enabled === true
	const jobsEnabled = jobs.enabled === true
	return (
		<div className="flex flex-col gap-5">
			<StepCard
				title="Sonde HTTP"
				description="Sonde le domaine de chaque lead, ses hôtes connus et les sous-domaines supplémentaires avec les empreintes ci-dessous."
				switchLabel="Activer la sonde HTTP"
				enabled={probeEnabled}
				onEnabled={(enabled) => onEnabled('http_probe', enabled)}
			>
				<div
					className={cn('flex flex-col gap-4', !probeEnabled && 'opacity-70')}
				>
					{field('http_probe', 'fingerprints', probe)}
					{field('http_probe', 'extra_subdomains', probe)}
					{field('http_probe', 'excluded_products', probe)}
				</div>
			</StepCard>
			<StepCard
				title="Offres d'emploi de l'entreprise"
				description="Trouve la page carrières ou la plateforme de recrutement de chaque lead à partir de son domaine et lit ses offres."
				switchLabel="Activer les offres d'emploi de l'entreprise"
				enabled={jobsEnabled}
				onEnabled={(enabled) => onEnabled('company_jobs', enabled)}
			>
				<div
					className={cn('flex flex-col gap-4', !jobsEnabled && 'opacity-70')}
				>
					{field('company_jobs', 'keywords', jobs)}
					<p className="text-xs text-muted-foreground">
						{keywordUnion.length > 0
							? `Utilisés aujourd'hui : ${keywordUnion.join(', ')}`
							: "Aucun agent de recherche activé n'a de mot-clé pour le moment."}
					</p>
					<details open={advancedOpen} className="rounded-md border px-3 py-2">
						<summary className="cursor-pointer text-sm font-medium">
							Avancé
						</summary>
						<div className="mt-3">
							{field('company_jobs', 'accept_min_keywords', jobs)}
						</div>
					</details>
				</div>
			</StepCard>
			<StepCard
				title="Dossier entreprise"
				description="Rassemble le site et les données officielles de l'entreprise de chaque lead."
			>
				<p className="text-sm">
					Toujours actif : aucun réglage. Le dossier apparaît dans la carte
					Entreprise de la page de chaque lead, avec la date de sa dernière mise
					à jour.{' '}
					<a
						href="/leads"
						className="font-medium text-primary underline-offset-4 hover:underline"
					>
						Voir les leads
					</a>
				</p>
			</StepCard>
		</div>
	)
}
