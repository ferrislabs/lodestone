import { describe, expect, it } from 'vitest'

import { buildMailLink, draftAsText, MAILTO_MAX_LENGTH } from './mail'

describe('draftAsText', () => {
	it('joins the subject and the body with a blank line', () => {
		expect(draftAsText('Hello', 'Body text')).toBe('Hello\n\nBody text')
	})
})

describe('buildMailLink', () => {
	it('carries the address, the subject and the body', () => {
		const link = buildMailLink({
			address: 'cto@acme.test',
			subject: 'Hello',
			body: 'Body',
		})
		expect(link).toEqual({
			href: 'mailto:cto@acme.test?subject=Hello&body=Body',
			bodyIncluded: true,
		})
	})

	it('encodes line breaks as %0D%0A', () => {
		const link = buildMailLink({
			address: 'cto@acme.test',
			subject: 'S',
			body: 'one\ntwo\n\nthree',
		})
		expect(link.href).toBe(
			'mailto:cto@acme.test?subject=S&body=one%0D%0Atwo%0D%0A%0D%0Athree',
		)
	})

	it('does not double the carriage return of Windows line breaks', () => {
		const link = buildMailLink({
			address: 'cto@acme.test',
			subject: 'S',
			body: 'one\r\ntwo',
		})
		expect(link.href).toBe('mailto:cto@acme.test?subject=S&body=one%0D%0Atwo')
	})

	it('encodes special characters of the subject and the body', () => {
		const link = buildMailLink({
			address: 'cto@acme.test',
			subject: 'Q&A? #1 café',
			body: 'a&b=c? #d é',
		})
		expect(link.href).toBe(
			'mailto:cto@acme.test?subject=Q%26A%3F%20%231%20caf%C3%A9&body=a%26b%3Dc%3F%20%23d%20%C3%A9',
		)
	})

	it('keeps the at sign and the plus of the address readable', () => {
		const link = buildMailLink({
			address: 'jean+lead@acme.test',
			subject: 'S',
			body: 'B',
		})
		expect(link.href).toBe('mailto:jean+lead@acme.test?subject=S&body=B')
	})

	it('drops the body when the link would exceed the limit', () => {
		const link = buildMailLink({
			address: 'cto@acme.test',
			subject: 'Sujet é',
			body: 'a'.repeat(MAILTO_MAX_LENGTH),
		})
		expect(link).toEqual({
			href: 'mailto:cto@acme.test?subject=Sujet%20%C3%A9',
			bodyIncluded: false,
		})
	})

	it('keeps the body when the link is exactly at the limit', () => {
		const prefix = 'mailto:cto@acme.test?subject=S&body='
		const link = buildMailLink({
			address: 'cto@acme.test',
			subject: 'S',
			body: 'a'.repeat(MAILTO_MAX_LENGTH - prefix.length),
		})
		expect(link.href).toHaveLength(MAILTO_MAX_LENGTH)
		expect(link.bodyIncluded).toBe(true)
	})

	it('drops the body one character above the limit', () => {
		const prefix = 'mailto:cto@acme.test?subject=S&body='
		const link = buildMailLink({
			address: 'cto@acme.test',
			subject: 'S',
			body: 'a'.repeat(MAILTO_MAX_LENGTH - prefix.length + 1),
		})
		expect(link).toEqual({
			href: 'mailto:cto@acme.test?subject=S',
			bodyIncluded: false,
		})
	})
})
