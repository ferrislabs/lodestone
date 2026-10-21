import { ModuleStatusChip } from '#/components/ecosystem/module-status-chip'
import { ProviderNotReady } from '#/components/ecosystem/provider-not-ready'
import type { FormContext } from '#/components/schema-form/context'
import { ObjectFields } from '#/components/schema-form/fields'
import { Button } from '#/components/ui/button'
import { Switch } from '#/components/ui/switch'
import type { JsonObject } from '#/lib/schema-form'
import { ProvenanceLink } from '#/pages/leads/ui/provenance-link'
import {
	customFieldSchema,
	enrichmentFieldSchema,
	type ModuleCard,
} from '#/pages/profiles/lib/enrichment'
import {
	type EcosystemTestActions,
	EcosystemTestPanel,
	type EcosystemTestState,
} from '#/pages/profiles/ui/ecosystem-test-panel'

export type ModulesState = 'loading' | 'error' | 'ready'

interface EnrichmentEditorProps {
	context: FormContext
	enrichment: JsonObject
	custom: JsonObject
	enabled: boolean
	cards: readonly ModuleCard[]
	modulesState: ModulesState
	keywordUnion: readonly string[]
	advancedOpen: boolean
	test: EcosystemTestState | null
	onEnabled: (enabled: boolean) => void
	onTest: (module: string) => void
	testActions: EcosystemTestActions
}

const CARD = 'flex min-w-0 flex-col gap-3 rounded-xl border bg-card p-5'

interface ModuleCardViewProps {
	card: ModuleCard
	test: EcosystemTestState | null
	onTest: (module: string) => void
	testActions: EcosystemTestActions
}

function ModuleCardView({
	card,
	test,
	onTest,
	testActions,
}: ModuleCardViewProps) {
	const testing = test !== null && test.module === card.module
	return (
		<section aria-label={card.label} data-module={card.module} className={CARD}>
			<div className="flex items-start justify-between gap-3">
				<div className="flex min-w-0 flex-col gap-1">
					<span className="text-xs font-medium uppercase text-muted-foreground">
						{card.label}
					</span>
					<h3 className="text-base font-semibold">{card.title}</h3>
				</div>
				<ModuleStatusChip label={card.chip.label} tone={card.chip.tone} />
			</div>
			<p className="text-sm text-muted-foreground">{card.learns}</p>
			{card.providerLink ? <ProviderNotReady href={card.providerLink} /> : null}
			<dl className="grid grid-cols-3 gap-2 text-sm">
				{card.stats.map((stat) => (
					<div key={stat.label} className="flex flex-col">
						<dt className="text-xs text-muted-foreground">{stat.label}</dt>
						<dd className="font-medium">{stat.value}</dd>
					</div>
				))}
			</dl>
			<div className="flex flex-col gap-0.5 text-sm">
				<span className="text-xs text-muted-foreground">Dernier exemple</span>
				{card.example ? (
					<>
						<span>
							{card.example.text}{' '}
							<span className="text-muted-foreground">
								({card.example.domain})
							</span>
						</span>
						<ProvenanceLink provenance={card.example.sourceUrl} />
					</>
				) : (
					<span className="text-muted-foreground">
						Rien sur les 7 derniers jours.
					</span>
				)}
			</div>
			<div className="flex flex-wrap items-center justify-between gap-2">
				<span className="text-xs text-muted-foreground">{card.refresh}</span>
				{card.canTest && !testing ? (
					<Button
						variant="outline"
						size="sm"
						onClick={() => onTest(card.module)}
					>
						Tester sur un lead
					</Button>
				) : null}
			</div>
			{testing && test ? (
				<EcosystemTestPanel test={test} actions={testActions} />
			) : null}
		</section>
	)
}

export function EnrichmentEditor({
	context,
	enrichment,
	custom,
	enabled,
	cards,
	modulesState,
	keywordUnion,
	advancedOpen,
	test,
	onEnabled,
	onTest,
	testActions,
}: EnrichmentEditorProps) {
	const customField = (key: string) => (
		<ObjectFields
			context={context}
			schema={customFieldSchema(key)}
			path={['enrichment', 'custom']}
			value={custom}
		/>
	)
	return (
		<div className="flex flex-col gap-5">
			<section
				aria-label="Inspection de l'écosystème"
				className="flex items-start justify-between gap-4 rounded-xl border bg-card p-5"
			>
				<div className="flex flex-col gap-1">
					<h2 className="text-base font-semibold">
						Inspection de l'écosystème
					</h2>
					<p className="text-sm text-muted-foreground">
						Chaque lead avec un domaine reçoit la même inspection, quel que soit
						le profil. Vos agents de qualification décident de ce qui compte.
					</p>
				</div>
				<Switch
					aria-label="Inspecter l'écosystème des leads"
					checked={enabled}
					onCheckedChange={onEnabled}
				/>
			</section>
			{modulesState === 'loading' ? (
				<p className="text-sm text-muted-foreground">Chargement des modules…</p>
			) : modulesState === 'error' ? (
				<p role="alert" className="text-sm text-destructive">
					Les modules n'ont pas pu être chargés.
				</p>
			) : (
				<div className="grid gap-4 xl:grid-cols-2">
					{cards.map((card) => (
						<ModuleCardView
							key={card.module}
							card={card}
							test={test}
							onTest={onTest}
							testActions={testActions}
						/>
					))}
				</div>
			)}
			<section aria-label="Mots-clés à mettre en avant" className={CARD}>
				<ObjectFields
					context={context}
					schema={enrichmentFieldSchema('highlight_keywords')}
					path={['enrichment']}
					value={enrichment}
				/>
				<p className="text-xs text-muted-foreground">
					{keywordUnion.length > 0
						? `Sans mot-clé ici, ceux des agents de recherche : ${keywordUnion.join(', ')}`
						: "Aucun agent de recherche activé n'a de mot-clé pour le moment."}
				</p>
			</section>
			<details open={advancedOpen} className="rounded-xl border px-5 py-3">
				<summary className="cursor-pointer text-sm font-medium">Avancé</summary>
				<div className="mt-3 flex flex-col gap-4">
					<p className="text-sm text-muted-foreground">
						Empreintes propres à ce profil, sondées sur le site de chaque lead
						avec les sous-domaines ci-dessous. Une correspondance apparaît dans
						« Site et stack technique » et devient un signal.
					</p>
					{customField('fingerprints')}
					{customField('extra_subdomains')}
					{customField('excluded_products')}
				</div>
			</details>
		</div>
	)
}
