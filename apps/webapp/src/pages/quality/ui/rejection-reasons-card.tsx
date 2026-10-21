import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from '#/components/ui/card'
import { formatNumber } from '#/lib/format'
import type { ProfileQuality } from '#/pages/quality/lib/types'
import { rejectionReasonLabel } from '#/pages/rejection/lib/reasons'

interface RejectionReasonsCardProps {
	profiles: readonly ProfileQuality[]
}

export function RejectionReasonsCard({ profiles }: RejectionReasonsCardProps) {
	const withReasons = profiles.filter(
		(profile) => profile.rejection_reasons.length > 0,
	)
	return (
		<Card aria-label="Raisons de rejet">
			<CardHeader>
				<CardTitle>Raisons de rejet</CardTitle>
				<CardDescription>
					Pourquoi les leads rejetés l'ont été, par profil. Une raison qui
					revient souvent désigne un critère à ajouter au profil.
				</CardDescription>
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				{withReasons.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucune raison de rejet enregistrée sur cette période.
					</p>
				) : (
					withReasons.map((profile) => (
						<section
							key={profile.profile_id}
							aria-label={`Raisons de rejet de ${profile.name}`}
							className="flex flex-col gap-1.5"
						>
							<h3 className="text-sm font-medium">{profile.name}</h3>
							<ul className="flex flex-col gap-1 text-sm">
								{profile.rejection_reasons.map((row) => (
									<li
										key={row.reason}
										className="flex items-baseline justify-between gap-4"
									>
										<span>{rejectionReasonLabel(row.reason)}</span>
										<span className="tabular-nums">
											{formatNumber(row.count)}
										</span>
									</li>
								))}
							</ul>
						</section>
					))
				)}
			</CardContent>
		</Card>
	)
}
