import { Button } from '#/components/ui/button'
import { buildMailLink, draftAsText } from '#/pages/drafts/lib/mail'

const BODY_TOO_LONG_HINT =
	'Le corps est trop long pour le lien : collez-le avec « Copier le corps ».'

interface DraftActionsProps {
	subject: string
	body: string
	contactAddress: string | null
	onCopy: (text: string) => void
}

export function DraftActions({
	subject,
	body,
	contactAddress,
	onCopy,
}: DraftActionsProps) {
	const mail =
		contactAddress === null
			? null
			: buildMailLink({ address: contactAddress, subject, body })
	return (
		<div className="flex flex-col gap-2" data-section="draft-actions">
			<div className="flex flex-wrap items-center gap-2">
				<Button
					type="button"
					variant="outline"
					size="sm"
					onClick={() => onCopy(subject)}
				>
					Copier l'objet
				</Button>
				<Button
					type="button"
					variant="outline"
					size="sm"
					onClick={() => onCopy(body)}
				>
					Copier le corps
				</Button>
				<Button
					type="button"
					variant="outline"
					size="sm"
					onClick={() => onCopy(draftAsText(subject, body))}
				>
					Copier le brouillon
				</Button>
				{mail ? (
					<Button variant="outline" size="sm" asChild>
						<a href={mail.href}>Ouvrir dans la messagerie</a>
					</Button>
				) : null}
			</div>
			{mail && !mail.bodyIncluded ? (
				<p className="text-xs text-muted-foreground">{BODY_TOO_LONG_HINT}</p>
			) : null}
		</div>
	)
}
