import { useState } from 'react'

import {
	BUDGET_CENTS_DEFAULT,
	SAMPLE_SIZE_DEFAULT,
} from '#/pages/profiles/lib/trials'

export function useTrialLauncher() {
	const [open, setOpen] = useState(false)
	const [sampleSize, setSampleSize] = useState(String(SAMPLE_SIZE_DEFAULT))
	const [budgetCents, setBudgetCents] = useState(String(BUDGET_CENTS_DEFAULT))
	const [message, setMessage] = useState<string | null>(null)
	return {
		open,
		sampleSize,
		budgetCents,
		message,
		show: () => {
			setMessage(null)
			setOpen(true)
		},
		close: () => {
			setMessage(null)
			setOpen(false)
		},
		fail: setMessage,
		setSampleSize: (value: string) => {
			setMessage(null)
			setSampleSize(value)
		},
		setBudgetCents: (value: string) => {
			setMessage(null)
			setBudgetCents(value)
		},
	}
}
