import { Card, CardContent, CardHeader, CardTitle } from '#/components/ui/card'
import { contactKindLabel, contactMethodLabel } from '#/lib/labels'
import { formatRatio } from '#/pages/leads/lib/format'
import type { Contact } from '#/pages/leads/lib/types'
import { ProvenanceLink } from './provenance-link'

interface ContactsListProps {
	contacts: readonly Contact[]
}

export function ContactsList({ contacts }: ContactsListProps) {
	return (
		<Card>
			<CardHeader>
				<CardTitle>Contacts ({contacts.length})</CardTitle>
			</CardHeader>
			<CardContent>
				{contacts.length === 0 ? (
					<p className="text-sm text-muted-foreground">Aucun contact trouvé.</p>
				) : (
					<ul className="flex flex-col divide-y">
						{contacts.map((contact) => (
							<li key={contact.id} className="flex flex-col gap-1 py-3">
								<span className="font-medium">{contact.address}</span>
								<div className="flex flex-wrap gap-x-4 text-xs text-muted-foreground">
									<span>Méthode : {contactMethodLabel(contact.method)}</span>
									<span>Type : {contactKindLabel(contact.kind)}</span>
									<span>Confiance : {formatRatio(contact.confidence)}</span>
								</div>
								<ProvenanceLink provenance={contact.provenance} />
							</li>
						))}
					</ul>
				)}
			</CardContent>
		</Card>
	)
}
