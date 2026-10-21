import { Button } from '#/components/ui/button'
import { QUALIFICATION_TEMPLATES } from '#/pages/profiles/lib/qualification'

interface QualificationPickerProps {
	onPick: (templateId: string) => void
	onCancel: () => void
}

export function QualificationPicker({
	onPick,
	onCancel,
}: QualificationPickerProps) {
	return (
		<section
			aria-label="Choisir un modèle d'agent de qualification"
			className="flex flex-col gap-3 rounded-md border bg-muted/20 p-3"
		>
			<div className="flex items-center justify-between">
				<p className="text-sm font-semibold">Quel angle doit-il évaluer ?</p>
				<Button type="button" variant="ghost" size="sm" onClick={onCancel}>
					Annuler
				</Button>
			</div>
			<ul className="grid gap-2 md:grid-cols-4">
				{QUALIFICATION_TEMPLATES.map((template) => (
					<li key={template.id}>
						<button
							type="button"
							className="flex h-full w-full flex-col gap-0.5 rounded-md border bg-background p-3 text-left hover:border-primary"
							onClick={() => onPick(template.id)}
						>
							<span className="text-sm font-medium">{template.label}</span>
							<span className="text-xs text-muted-foreground">
								{template.description}
							</span>
						</button>
					</li>
				))}
			</ul>
		</section>
	)
}
