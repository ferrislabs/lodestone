import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import type { PipelineStage } from '#/pages/leads/lib/timeline'
import { PipelineStepper } from './pipeline-stepper'

afterEach(cleanup)

const NOW = Date.parse('2026-10-09T10:00:00Z')

const STAGES: PipelineStage[] = [
	{
		stage: 'detection',
		status: 'done',
		label: 'Détection',
		reason: '2 signaux',
	},
	{
		stage: 'inspection',
		status: 'running',
		label: 'Inspection',
		reason: 'Module Site et stack technique en cours',
		since: '2026-10-09T09:59:00Z',
	},
	{
		stage: 'qualification',
		status: 'waiting',
		label: 'Qualification',
		reason: "Échec de l'appel au modèle de l'agent stack (tentative 1)",
		next_action: "Nouvel essai de l'appel",
		next_at: '2026-10-09T10:05:00Z',
	},
	{ stage: 'scoring', status: 'skipped', label: 'Scoring' },
	{ stage: 'synthesis', status: 'waiting', label: 'Synthèse' },
	{ stage: 'draft', status: 'waiting', label: 'Brouillon' },
	{ stage: 'decision', status: 'blocked', label: 'Décision' },
]

function stageItem(stage: string): HTMLElement {
	const item = document.querySelector<HTMLElement>(`[data-stage="${stage}"]`)
	if (!item) throw new Error(`no stage ${stage}`)
	return item
}

describe('PipelineStepper', () => {
	it('shows the seven stages in order with their status', () => {
		render(
			<PipelineStepper
				stages={STAGES}
				current="inspection"
				now={NOW}
				error={null}
			/>,
		)
		const labels = Array.from(document.querySelectorAll('[data-stage]')).map(
			(item) => item.getAttribute('data-stage'),
		)
		expect(labels).toEqual([
			'detection',
			'inspection',
			'qualification',
			'scoring',
			'synthesis',
			'draft',
			'decision',
		])
		expect(stageItem('decision').getAttribute('data-status')).toBe('blocked')
		expect(stageItem('decision').textContent).toContain('Bloqué')
		expect(stageItem('scoring').textContent).toContain('Sans objet')
	})

	it('highlights the current stage with its reason', () => {
		render(
			<PipelineStepper
				stages={STAGES}
				current="inspection"
				now={NOW}
				error={null}
			/>,
		)
		expect(stageItem('inspection').getAttribute('aria-current')).toBe('step')
		expect(stageItem('detection').getAttribute('aria-current')).toBeNull()
		expect(
			screen.getByText(/en cours : Module Site et stack technique en cours/),
		).toBeTruthy()
	})

	it('announces the next action and its time for a waiting stage', () => {
		render(
			<PipelineStepper
				stages={STAGES}
				current="qualification"
				now={NOW}
				error={null}
			/>,
		)
		expect(
			screen.getByText(
				/Prochaine étape : Nouvel essai de l'appel à \d{2}:\d{2}/,
			),
		).toBeTruthy()
		expect(
			screen.getByText(/en attente : Échec de l'appel au modèle/),
		).toBeTruthy()
	})

	it('says when everything is settled and reports a loading error', () => {
		render(
			<PipelineStepper
				stages={STAGES.map((stage) => ({ ...stage, status: 'done' as const }))}
				current={null}
				now={NOW}
				error={null}
			/>,
		)
		expect(screen.getByText('Traitement terminé pour ce lead.')).toBeTruthy()
		cleanup()
		render(
			<PipelineStepper stages={[]} current={null} now={NOW} error="boom" />,
		)
		expect(screen.getByRole('alert').textContent).toContain('boom')
	})
})
