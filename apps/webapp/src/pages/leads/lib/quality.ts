const WARNING_LABELS: Readonly<Record<string, string>> = {
	repetition: 'répétitions',
	too_long: 'texte trop long',
	restated_figures: 'pourcentage ou verdict répétés avec un chiffre',
	dropped_quotes: 'citations non retrouvées écartées',
}

export function qualityWarningLabel(warning: string): string {
	return WARNING_LABELS[warning] ?? warning
}

export function qualityWarningMessage(
	warnings: readonly string[] | undefined,
): string | null {
	if (warnings === undefined || warnings.length === 0) return null
	return `Synthèse de qualité incertaine : ${warnings.map(qualityWarningLabel).join(', ')}`
}
