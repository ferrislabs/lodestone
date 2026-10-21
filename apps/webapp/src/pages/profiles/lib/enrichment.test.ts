import { describe, expect, it } from 'vitest'

import type { JsonObject } from '#/lib/schema-form'
import { newAgent } from '#/pages/profiles/lib/agents'
import {
	ENRICHMENT_WAIT_MINUTES,
	enrichmentFieldSchema,
	enrichmentStep,
	searchKeywordUnion,
	stepEnabled,
	stepHasProblem,
	withStepEnabled,
} from '#/pages/profiles/lib/enrichment'

function search(keywords: string[], enabled = true): JsonObject {
	return { ...newAgent('search', [], ['boamp']), keywords, enabled }
}

describe('enrichment steps', () => {
	it('reads a missing section as disabled defaults', () => {
		expect(stepEnabled({}, 'http_probe')).toBe(false)
		expect(enrichmentStep({}, 'company_jobs')).toEqual({
			accept_min_keywords: 2,
			enabled: false,
			keywords: [],
		})
	})

	it('enables a step and keeps the other settings', () => {
		const value: JsonObject = {
			enrichment: {
				http_probe: {
					enabled: false,
					fingerprints: [],
					excluded_products: ['FerrisKey'],
					extra_subdomains: ['auth'],
				},
			},
		}
		const next = withStepEnabled(value, 'http_probe', true)
		expect(stepEnabled(next, 'http_probe')).toBe(true)
		expect(enrichmentStep(next, 'http_probe').excluded_products).toEqual([
			'FerrisKey',
		])
		expect(stepEnabled(next, 'company_jobs')).toBe(false)
	})

	it('fills the defaults of a step enabled from an empty section', () => {
		const next = withStepEnabled({}, 'company_jobs', true)
		expect(enrichmentStep(next, 'company_jobs')).toEqual({
			accept_min_keywords: 2,
			enabled: true,
			keywords: [],
		})
	})

	it('picks one field of a step schema', () => {
		const schema = enrichmentFieldSchema('http_probe', 'extra_subdomains')
		expect(Object.keys(schema.properties ?? {})).toEqual(['extra_subdomains'])
		expect(
			Object.keys(
				enrichmentFieldSchema('company_jobs', 'accept_min_keywords')
					.properties ?? {},
			),
		).toEqual(['accept_min_keywords'])
	})

	it('flags problems under a step field', () => {
		const problems = [
			{ path: 'enrichment.company_jobs.accept_min_keywords', message: 'x' },
		]
		expect(
			stepHasProblem(problems, 'company_jobs', 'accept_min_keywords'),
		).toBe(true)
		expect(stepHasProblem(problems, 'company_jobs', 'keywords')).toBe(false)
	})

	it('waits thirty minutes by default', () => {
		expect(ENRICHMENT_WAIT_MINUTES).toBe(30)
	})
})

describe('searchKeywordUnion', () => {
	it('merges the keywords of the enabled search agents once', () => {
		const value: JsonObject = {
			agents: [
				search(['Keycloak', 'SSO']),
				search(['sso', 'IAM']),
				search(['ignored'], false),
				newAgent('certificates', []),
			],
		}
		expect(searchKeywordUnion(value)).toEqual(['Keycloak', 'SSO', 'IAM'])
	})
})
