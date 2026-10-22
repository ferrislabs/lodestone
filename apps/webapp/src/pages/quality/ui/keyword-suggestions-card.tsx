import { Button } from '#/components/ui/button'
import {
	Card,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from '#/components/ui/card'
import {
	profileName,
	type SuggestionOutcome,
	spansSeveralProfiles,
	suggestionKey,
	suggestionSentence,
} from '#/pages/quality/lib/sourcing'
import type {
	AgentYield,
	KeywordSuggestion,
	ProfileSummary,
} from '#/pages/quality/lib/types'

interface KeywordSuggestionsCardProps {
	suggestions: readonly KeywordSuggestion[]
	agents: readonly AgentYield[]
	profiles: readonly ProfileSummary[]
	outcomes: Readonly<Record<string, SuggestionOutcome>>
	onApply: (suggestion: KeywordSuggestion) => void
}

export function KeywordSuggestionsCard({
	suggestions,
	agents,
	profiles,
	outcomes,
	onApply,
}: KeywordSuggestionsCardProps) {
	const showProfiles = spansSeveralProfiles(suggestions)
	return (
		<Card aria-label="Suggestions de mots-clés">
			<CardHeader>
				<CardTitle>Suggestions de mots-clés</CardTitle>
				<CardDescription>
					Règles fixes appliquées aux décisions humaines. Rien n'est modifié
					sans votre clic : appliquer crée une nouvelle révision du profil.
				</CardDescription>
			</CardHeader>
			<CardContent>
				<ul className="flex flex-col gap-3 text-sm">
					{suggestions.map((suggestion) => {
						const key = suggestionKey(suggestion)
						const outcome = outcomes[key]
						const name = showProfiles
							? profileName(profiles, suggestion.profile_id)
							: null
						return (
							<li
								key={key}
								data-suggestion={key}
								className="flex flex-col gap-1"
							>
								<div className="flex flex-wrap items-center justify-between gap-2">
									<span className="flex flex-col">
										<span>{suggestionSentence(suggestion, agents)}</span>
										{name === null ? null : (
											<span className="text-xs text-muted-foreground">
												{name}
											</span>
										)}
									</span>
									{outcome?.status === 'applied' ? (
										<span className="text-xs font-medium text-muted-foreground">
											Révision {outcome.revision} créée
										</span>
									) : (
										<Button
											type="button"
											variant="outline"
											size="sm"
											disabled={outcome?.status === 'pending'}
											onClick={() => onApply(suggestion)}
										>
											{outcome?.status === 'pending'
												? 'Application…'
												: 'Appliquer'}
										</Button>
									)}
								</div>
								{outcome?.status === 'failed' ? (
									<p role="alert" className="text-xs text-destructive">
										{outcome.message}
									</p>
								) : null}
							</li>
						)
					})}
				</ul>
			</CardContent>
		</Card>
	)
}
