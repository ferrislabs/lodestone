import { describe, expect, it } from 'vitest'

import {
	activityText,
	companyNotice,
	companyState,
	companyStateLabel,
	countryBasisLabel,
	formatDay,
	formatSiren,
	headOfficeText,
	pageLabel,
} from './company'
import type { Company, CompanyOfficial } from './types'

const official: CompanyOfficial = {
	siren: '552081317',
	legal_name: 'ELECTRICITE DE FRANCE',
	naf_code: '35.11Z',
	naf_label: "Production et distribution d'électricité",
	headcount_range: '10 000 salariés et plus',
	head_office_city: 'PARIS',
	head_office_postcode: '75008',
	head_office_country: 'France',
	created_on: '1955-01-01',
	annuaire_url:
		'https://annuaire-entreprises.data.gouv.fr/entreprise/552081317',
}

function company(overrides: Partial<Company> = {}): Company {
	return {
		state: 'complete',
		siren: '552081317',
		official,
		pages: [],
		built_at: '2026-10-06T10:00:00Z',
		next_attempt_at: '2027-01-04T10:00:00Z',
		error: null,
		...overrides,
	}
}

describe('company dossier helpers', () => {
	it('groups a SIREN by three digits and leaves anything else untouched', () => {
		expect(formatSiren('552081317')).toBe('552 081 317')
		expect(formatSiren('55208')).toBe('55208')
	})

	it('reads a missing or unknown state as pending', () => {
		expect(companyState(null)).toBe('pending')
		expect(companyState(company({ state: 'weird' }))).toBe('pending')
		expect(companyStateLabel(companyState(company()))).toBe('Complète')
		expect(companyStateLabel('unreachable')).toBe('Inaccessible')
	})

	it('joins the activity code and its label', () => {
		expect(activityText(official)).toBe(
			"35.11Z · Production et distribution d'électricité",
		)
		expect(activityText({ ...official, naf_code: null, naf_label: null })).toBe(
			null,
		)
	})

	it('writes the head office as postcode, city and country', () => {
		expect(headOfficeText(official)).toBe('75008 PARIS, France')
		expect(
			headOfficeText({
				...official,
				head_office_postcode: null,
				head_office_city: null,
			}),
		).toBe('France')
	})

	it('formats a creation day in French', () => {
		expect(formatDay('1955-01-01')).toBe('1 janvier 1955')
		expect(formatDay('nope')).toBe('nope')
	})

	it('explains why official data is missing', () => {
		expect(companyNotice(company())).toBeNull()
		expect(companyNotice(null)).toContain('pas encore été faite')
		expect(companyNotice(company({ state: 'unreachable' }))).toContain(
			'inaccessible',
		)
		expect(
			companyNotice(company({ state: 'partial', siren: null, official: null })),
		).toContain('SIREN introuvable')
		expect(
			companyNotice(
				company({ state: 'partial', siren: '552081317', official: null }),
			),
		).toContain('552 081 317')
	})

	it('shortens a page address to host and path', () => {
		expect(pageLabel('https://acme.fr/')).toBe('acme.fr')
		expect(pageLabel('https://acme.fr/a-propos')).toBe('acme.fr/a-propos')
		expect(pageLabel('not a url')).toBe('not a url')
	})

	it('names the basis of the probable country', () => {
		expect(countryBasisLabel('head_office')).toBe('siège')
		expect(countryBasisLabel('tld')).toBe('extension du domaine')
		expect(countryBasisLabel(null)).toBe('')
	})
})
