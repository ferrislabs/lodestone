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
	discardSuggestion,
	formatRatio,
	insufficientMessage,
	reliableSuggestion,
} from '#/pages/quality/lib/format'
import type { Quality } from '#/pages/quality/lib/types'

interface SuggestionsCardProps {
	profiles: Quality['profiles']
	onOpenProfile: (profileId: string) => void
}

export function SuggestionsCard({
	profiles,
	onOpenProfile,
}: SuggestionsCardProps) {
	return (
		<Card aria-label="Suggestions de seuils">
			<CardHeader>
				<CardTitle>Suggestions de seuils</CardTitle>
				<CardDescription>
					Indicatif seulement : Lodestone ne modifie jamais un seuil de
					lui-même. Le seuil « Fiable » retenu maximise le score F1 des leads
					validés ; le seuil « Écarté » est la valeur sous laquelle au plus 5 %
					des leads décidés ont été validés. Il faut au moins 20 décisions.
				</CardDescription>
			</CardHeader>
			<CardContent className="flex flex-col gap-4">
				{profiles.length === 0 ? (
					<p className="text-sm text-muted-foreground">
						Aucun profil avec des décisions sur la période.
					</p>
				) : null}
				{profiles.map((profile) => (
					<section
						key={profile.profile_id}
						aria-label={`Suggestions pour ${profile.name}`}
						className="flex flex-col gap-1.5 rounded-lg border p-4"
					>
						<div className="flex flex-wrap items-center justify-between gap-2">
							<h3 className="font-semibold">{profile.name}</h3>
							<Button
								type="button"
								variant="outline"
								size="sm"
								onClick={() => onOpenProfile(profile.profile_id)}
							>
								Ouvrir l'onglet Qualification
							</Button>
						</div>
						{profile.suggestion.ready ? (
							<ul className="flex flex-col gap-0.5 text-sm">
								<li>{reliableSuggestion(profile)}</li>
								<li>{discardSuggestion(profile)}</li>
								<li className="text-muted-foreground">
									Calculé sur {profile.suggestion.labelled} décisions.
								</li>
							</ul>
						) : (
							<p className="text-sm text-muted-foreground">
								{insufficientMessage(profile.suggestion)}
							</p>
						)}
					</section>
				))}
				{profiles.length > 0 ? (
					<Table>
						<TableHeader>
							<TableRow>
								<TableHead>Profil</TableHead>
								<TableHead className="text-right">Validés</TableHead>
								<TableHead className="text-right">Rejetés</TableHead>
								<TableHead className="text-right">Précision</TableHead>
							</TableRow>
						</TableHeader>
						<TableBody>
							{profiles.map((profile) => (
								<TableRow key={profile.profile_id}>
									<TableCell>{profile.name}</TableCell>
									<TableCell className="text-right tabular-nums">
										{profile.validated}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{profile.rejected}
									</TableCell>
									<TableCell className="text-right tabular-nums">
										{formatRatio(profile.precision)}
									</TableCell>
								</TableRow>
							))}
						</TableBody>
					</Table>
				) : null}
			</CardContent>
		</Card>
	)
}
