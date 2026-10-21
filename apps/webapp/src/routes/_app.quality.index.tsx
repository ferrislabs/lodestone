import { createFileRoute } from '@tanstack/react-router'

import { QualityFeature } from '#/pages/quality/feature/quality-feature'
import { qualitySearchSchema } from '#/pages/quality/lib/period'

export const Route = createFileRoute('/_app/quality/')({
	validateSearch: qualitySearchSchema,
	head: () => ({ meta: [{ title: 'Qualité · Lodestone' }] }),
	component: QualityFeature,
})
