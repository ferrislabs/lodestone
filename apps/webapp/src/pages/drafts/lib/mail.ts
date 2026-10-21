export const MAILTO_MAX_LENGTH = 1800

export function draftAsText(subject: string, body: string): string {
	return `${subject}\n\n${body}`
}

export interface MailLink {
	href: string
	bodyIncluded: boolean
}

interface MailLinkInput {
	address: string
	subject: string
	body: string
}

export function buildMailLink({
	address,
	subject,
	body,
}: MailLinkInput): MailLink {
	const recipient = encodeURIComponent(address)
		.replaceAll('%40', '@')
		.replaceAll('%2B', '+')
	const subjectOnly = `mailto:${recipient}?subject=${encodeURIComponent(subject)}`
	const bodyWithCrLf = body.replace(/\r?\n/g, '\r\n')
	const full = `${subjectOnly}&body=${encodeURIComponent(bodyWithCrLf)}`
	if (full.length > MAILTO_MAX_LENGTH) {
		return { href: subjectOnly, bodyIncluded: false }
	}
	return { href: full, bodyIncluded: true }
}
