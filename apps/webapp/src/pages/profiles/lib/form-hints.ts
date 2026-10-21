import type { UiHints } from '#/lib/schema-form'
import {
	FIELD_COPY,
	FORM_MESSAGES,
	VARIANT_TITLES,
} from '#/pages/profiles/lib/field-copy'
import { DEFAULT_AGGREGATOR_PROMPT } from '#/pages/profiles/lib/qualification-templates'

export const PROFILE_HINTS: UiHints = {
	hidden: ['schema_version'],
	textarea: [
		'identity.offer',
		'identity.target_customer',
		'qualification_agents.system_prompt',
		'scoring_agents.system_prompt',
		'aggregator.system_prompt',
	],
	defaults: {
		schema_version: 6,
		'aggregator.system_prompt': DEFAULT_AGGREGATOR_PROMPT,
		'enrichment.http_probe.fingerprints.matchers.equals': 200,
	},
	labels: {
		ats: 'ATS',
		boamp: 'BOAMP',
		decp: 'DECP',
		ted: 'TED',
		dns_cname: 'DNS CNAME',
		dns: 'DNS',
		http_probe: 'Sonde HTTP',
		cname: 'CNAME',
	},
	fields: FIELD_COPY,
	variants: VARIANT_TITLES,
	messages: FORM_MESSAGES,
	layout: {
		agents: { row: ['keywords', 'exclusions'] },
		'enrichment.http_probe.fingerprints': {
			row: ['id', 'path', 'product'],
			collapsed: {
				title: 'Version et dette technique',
				keys: ['version', 'debt_markers'],
			},
		},
	},
	optionLabels: {
		language: { fr: 'Français', en: 'English' },
	},
	choices: {
		'agents.accept_min_keywords': { min: 1, max: 5 },
		'enrichment.company_jobs.accept_min_keywords': { min: 1, max: 5 },
	},
}
