import { Button } from '#/components/ui/button'
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from '#/components/ui/card'
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from '#/components/ui/table'
import {
	calibrationKindLabel,
	calibrationStatusLabel,
	formatBrier,
} from '#/pages/quality/lib/calibration'
import type {
	AgentCalibration,
	Calibration,
	ProfileCalibration,
} from '#/pages/quality/lib/types'
import { ReliabilityCurve } from '#/pages/quality/ui/reliability-curve'

interface CalibrationCardProps {
	calibration: Calibration
	profiles: readonly ProfileCalibration[]
	running: boolean
	notice: string | null
	errorMessage: string | null
	onRecalculate: () => void
}

function AgentRow({
	agent,
	required,
}: {
	agent: AgentCalibration
	required: number
}) {
	return (
		<TableRow
			data-calibration-agent={agent.agent_id}
			data-kind={agent.kind}
			data-status={agent.status}
		>
			<TableCell>
				<div>{agent.name}</div>
				<div className="text-xs text-muted-foreground">
					{calibrationKindLabel(agent.kind)}
				</div>
			</TableCell>
			<TableCell className="text-right tabular-nums">{agent.n}</TableCell>
			<TableCell className="text-right tabular-nums">
				{agent.positives}
			</TableCell>
			<TableCell className="text-right tabular-nums">
				{formatBrier(agent.brier_before)}
			</TableCell>
			<TableCell className="text-right tabular-nums">
				{formatBrier(agent.brier_after)}
			</TableCell>
			<TableCell data-part="status">
				{calibrationStatusLabel(agent, required)}
			</TableCell>
		</TableRow>
	)
}

function ProfileSection({
	profile,
	required,
}: {
	profile: ProfileCalibration
	required: number
}) {
	const curves = profile.agents.filter((agent) => agent.reliability.length > 0)
	return (
		<section
			aria-label={`Calibration de ${profile.name}`}
			className="flex flex-col gap-3 rounded-lg border p-4"
		>
			<h3 className="font-semibold">{profile.name}</h3>
			{profile.agents.length === 0 ? (
				<p className="text-sm text-muted-foreground">
					Aucun agent de scoring ou de qualification dans ce profil.
				</p>
			) : (
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>Agent</TableHead>
							<TableHead className="text-right">n</TableHead>
							<TableHead className="text-right">Validés</TableHead>
							<TableHead className="text-right">Brier avant</TableHead>
							<TableHead className="text-right">Brier après</TableHead>
							<TableHead>État</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{profile.agents.map((agent) => (
							<AgentRow
								key={`${agent.kind}-${agent.agent_id}`}
								agent={agent}
								required={required}
							/>
						))}
					</TableBody>
				</Table>
			)}
			{curves.length > 0 ? (
				<div className="flex flex-wrap gap-6" data-part="curves">
					{curves.map((agent) => (
						<figure
							key={`${agent.kind}-${agent.agent_id}`}
							className="flex flex-col items-center gap-1"
						>
							<ReliabilityCurve
								label={`Courbe de fiabilité de ${agent.name}`}
								bins={agent.reliability}
							/>
							<figcaption className="text-xs text-muted-foreground">
								{agent.name} : prédit en abscisse, observé en ordonnée
							</figcaption>
						</figure>
					))}
				</div>
			) : null}
		</section>
	)
}

export function CalibrationCard({
	calibration,
	profiles,
	running,
	notice,
	errorMessage,
	onRecalculate,
}: CalibrationCardProps) {
	return (
		<Card aria-label="Calibration">
			<CardHeader>
				<div className="flex flex-wrap items-start justify-between gap-3">
					<div className="flex flex-col gap-1.5">
						<CardTitle>Calibration</CardTitle>
						<CardDescription>
							Les décisions humaines corrigent les probabilités des agents : une
							courbe monotone apprise chaque jour relie la probabilité annoncée
							à la fréquence réellement validée. Elle ne s'applique qu'à partir
							de {calibration.required} leads étiquetés, dont au moins{' '}
							{calibration.required_per_class} validés et{' '}
							{calibration.required_per_class} rejetés. Le score de Brier mesure
							l'erreur des probabilités : plus il est bas, mieux c'est.
						</CardDescription>
					</div>
					<Button
						type="button"
						variant="outline"
						size="sm"
						disabled={running}
						onClick={onRecalculate}
					>
						Recalculer maintenant
					</Button>
				</div>
				{notice ? (
					<output className="text-sm text-muted-foreground">{notice}</output>
				) : null}
				{errorMessage ? (
					<p role="alert" className="text-sm text-destructive">
						{errorMessage}
					</p>
				) : null}
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				{profiles.length === 0 ? (
					<p className="text-sm text-muted-foreground">Aucun profil.</p>
				) : null}
				{profiles.map((profile) => (
					<ProfileSection
						key={profile.profile_id}
						profile={profile}
						required={calibration.required}
					/>
				))}
			</CardContent>
		</Card>
	)
}
