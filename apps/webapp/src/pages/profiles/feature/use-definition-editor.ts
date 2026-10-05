import { useMemo, useState } from 'react'

import { type JsonObject, stableStringify } from '#/lib/schema-form'

export function useDefinitionEditor(initial: JsonObject) {
	const [override, setOverride] = useState<JsonObject | null>(null)
	const [confirmingReset, setConfirmingReset] = useState(false)
	const value = override ?? initial
	const dirty = useMemo(
		() =>
			override !== null &&
			stableStringify(override) !== stableStringify(initial),
		[override, initial],
	)
	return {
		value,
		dirty,
		set: setOverride,
		confirmingReset,
		requestReset: () => setConfirmingReset(true),
		cancelReset: () => setConfirmingReset(false),
		reset: () => {
			setOverride(null)
			setConfirmingReset(false)
		},
	}
}
