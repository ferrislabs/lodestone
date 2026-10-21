export const NEW_PROFILE_TEMPLATE = `{
  "schema_version": 2,
  "identity": {
    "offer": "",
    "target_customer": ""
  },
  "agents": [],
  "scoring": {
    "factors": []
  },
  "contact_ranking": {
    "preferred_local_parts": []
  },
  "llm": {
    "qualification_threshold": 50,
    "relevance_criteria": ""
  }
}
`

export function prettyDefinition(definition: Record<string, unknown>): string {
	return JSON.stringify(definition, null, 2)
}

export function saveResultMessage(previous: number, saved: number): string {
	return saved > previous
		? `Revision ${saved} created`
		: `No change, revision ${saved} kept`
}

export function exportFilename(name: string): string {
	const safe = name.replace(/[^A-Za-z0-9_-]/g, '_')
	return `${safe}.json`
}
