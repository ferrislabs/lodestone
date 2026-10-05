import type { UiHints } from '#/lib/schema-form'

export const PROFILE_HINTS: UiHints = {
	hidden: ['schema_version'],
	textarea: [
		'identity.offer',
		'identity.target_customer',
		'llm.relevance_criteria',
	],
	defaults: {
		schema_version: 1,
		'llm.qualification_threshold': 50,
		'sources.http_probe.fingerprints.matchers.equals': 200,
	},
	labels: {
		ats: 'ATS',
		boamp: 'BOAMP',
		decp: 'DECP',
		ted: 'TED',
		llm: 'LLM',
		dns_cname: 'DNS CNAME',
		http_probe: 'HTTP probe',
		cname: 'CNAME',
	},
}
