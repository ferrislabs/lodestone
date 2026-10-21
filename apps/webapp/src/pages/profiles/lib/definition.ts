export const NEW_PROFILE_TEMPLATE = `{
  "schema_version": 11,
  "identity": {
    "offer": "",
    "target_customer": ""
  },
  "agents": [],
  "qualification_agents": [],
  "scoring_agents": [],
  "aggregator": {
    "provider_id": null,
    "system_prompt": ""
  },
  "escalation": {
    "provider": null,
    "band_low": 40,
    "band_high": 70
  },
  "verdict": {
    "reliable_from": 70,
    "discard_below": 30,
    "calibration_floor": 0.05,
    "min_evidence": 0.5
  },
  "evidence": {
    "influence": 0.6,
    "reference_score": 50,
    "weights": {
      "volume": 1,
      "diversity": 1,
      "environment": 1,
      "strength": 1
    },
    "half_life_days": {
      "job_offer": 90,
      "public_tender": 180,
      "awarded_contract": 365,
      "saas_provider": 365,
      "product_detected": 365,
      "debt_marker": 365,
      "contact": 365,
      "role_contact_found": 365,
      "default": 180
    }
  },
  "scoring": {
    "factors": []
  },
  "contact_ranking": {
    "preferred_local_parts": []
  },
  "llm": {
    "qualification_threshold": 50,
    "relevance_criteria": "",
    "language": "fr"
  }
}
`

export function prettyDefinition(definition: Record<string, unknown>): string {
	return JSON.stringify(definition, null, 2)
}

export function saveResultMessage(previous: number, saved: number): string {
	return saved > previous
		? `Révision ${saved} créée`
		: `Aucun changement, révision ${saved} conservée`
}

export function exportFilename(name: string): string {
	const safe = name.replace(/[^A-Za-z0-9_-]/g, '_')
	return `${safe}.json`
}
