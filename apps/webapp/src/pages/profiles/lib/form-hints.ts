import type { UiHints } from '#/lib/schema-form'
import {
	FIELD_COPY,
	FORM_MESSAGES,
	VARIANT_TITLES,
} from '#/pages/profiles/lib/field-copy'

export const PROFILE_HINTS: UiHints = {
	hidden: ['schema_version'],
	textarea: [
		'identity.offer',
		'identity.target_customer',
		'llm.relevance_criteria',
		'agents.llm.relevance_criteria',
	],
	defaults: {
		schema_version: 3,
		'llm.qualification_threshold': 50,
		'scoring.factors.weight': 5,
		'agents.source.fingerprints.matchers.equals': 200,
	},
	labels: {
		ats: 'ATS',
		boamp: 'BOAMP',
		decp: 'DECP',
		ted: 'TED',
		llm: 'LLM',
		dns_cname: 'DNS CNAME',
		http_probe: 'Sonde HTTP',
		cname: 'CNAME',
	},
	fields: FIELD_COPY,
	variants: VARIANT_TITLES,
	messages: FORM_MESSAGES,
	layout: {
		'agents.source': { row: ['keywords', 'exclusions'] },
		'agents.source.fingerprints': {
			row: ['id', 'path', 'product'],
			collapsed: {
				title: 'Version et dette technique',
				keys: ['version', 'debt_markers'],
			},
		},
	},
	optionLabels: {
		'llm.language': { fr: 'Français', en: 'English' },
	},
	choices: {
		'agents.source.accept_min_keywords': { min: 1, max: 5 },
	},
}
